//! Regression for the Android "remote folders not shown" bug.
//!
//! When the LINUX app initiates the pairing toward Android, Android is the
//! pairing *responder*. At approval time it only knows the incoming connection's
//! source IP, and records the peer at that IP plus a DEFAULT_PORT (9847) guess.
//! If Linux actually serves on another port, that guess points at nothing, so
//! browsing for "remote folders" from Android silently returns an empty list.
//!
//! Android must refine the stored address to the one Linux advertises on mDNS,
//! after which the browse dials a live listener.
//!
//! This lives in its own test file (its own process) because the server registry
//! is process-global and keyed by the per-db autoincrement folder id, and each
//! test runs its own mDNS daemons; a sibling test file would otherwise share
//! those resources.

use ferrisync_core::api::{
    approve_pending_pairing, browse_peer_shared_folders, device_last_addr, init_engine,
    pair_with_device, pending_pairings, share_folder, start_server, stop_server,
};
use std::time::Duration;

fn get_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    std::thread::sleep(Duration::from_millis(50));
    port
}

#[tokio::test]
async fn responder_refines_peer_address_to_its_advertised_port() {
    let linux_dir = tempfile::tempdir().unwrap();
    let linux_data = tempfile::tempdir().unwrap();
    let android_dir = tempfile::tempdir().unwrap();
    let android_data = tempfile::tempdir().unwrap();
    let linux_port = get_available_port();
    let android_port = get_available_port();
    assert_ne!(linux_port, android_port, "ports must differ");

    // ── Linux app: own folder, published share, live server ──
    let linux = std::sync::Arc::new(
        init_engine(linux_data.path().to_str().unwrap().to_string())
            .await
            .unwrap(),
    );
    let linux_id = linux.current_device().id.clone();
    let linux_folder_id = linux
        .storage
        .add_sync_folder(
            linux_dir.path().to_str().unwrap(),
            &linux_id,
            "bidirectional",
        )
        .unwrap();
    let share_guid = linux
        .storage
        .folder_guid(linux_folder_id)
        .unwrap()
        .expect("linux folder has a guid");
    share_folder(&linux, linux_folder_id, "shared-notes".into()).unwrap();
    start_server(
        &linux,
        linux_port,
        linux_folder_id,
        linux_dir.path().to_str().unwrap().to_string(),
    )
    .await
    .unwrap();

    // ── Android app: needs a listener so it can be the pairing responder ──
    //
    // This process's server registry is global and keyed by the per-db
    // autoincrement folder id, so give Android a distinct id (its listener
    // folder is id 2) to avoid colliding with Linux's folder (id 1).
    let android = init_engine(android_data.path().to_str().unwrap().to_string())
        .await
        .unwrap();
    let android_id = android.current_device().id.clone();
    let android_dummy = tempfile::tempdir().unwrap();
    android
        .storage
        .add_sync_folder(
            android_dummy.path().to_str().unwrap(),
            &android_id,
            "bidirectional",
        )
        .unwrap();
    let android_folder_id = android
        .storage
        .add_sync_folder(
            android_dir.path().to_str().unwrap(),
            &android_id,
            "bidirectional",
        )
        .unwrap();
    start_server(
        &android,
        android_port,
        android_folder_id,
        android_dir.path().to_str().unwrap().to_string(),
    )
    .await
    .unwrap();

    // ── Linux initiates the pairing toward Android ──
    let linux_arc = std::sync::Arc::clone(&linux);
    let pair_task = tokio::spawn(async move {
        pair_with_device(&linux_arc, "127.0.0.1".into(), android_port).await
    });

    let mut approved = false;
    for _ in 0..150 {
        if let Ok(pending) = pending_pairings(&android) {
            if pending.iter().any(|(_, id)| id == &linux_id) {
                approve_pending_pairing(&android, linux_id.clone(), "linux-device".into()).unwrap();
                approved = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let pair_result = pair_task.await.unwrap();
    assert!(
        approved,
        "android never saw linux's device pairing (pair task: {pair_result:?})"
    );
    pair_result.unwrap();

    // ── Android must converge on linux's REAL advertised address ──
    // The refinement runs in a background mDNS probe after approval, so poll
    // briefly for convergence. Only the port is asserted: the advertised host
    // is the LAN address here (this sandbox's mDNS advertisement resolves to the
    // machine's primary interface) rather than loopback.
    let mut refined: Option<String> = None;
    for _ in 0..100 {
        if let Some(recorded) = device_last_addr(&android, linux_id.clone()).unwrap() {
            if recorded.ends_with(&format!(":{linux_port}")) {
                refined = Some(recorded);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let recorded = refined.unwrap_or_else(|| {
        panic!(
            "android should store linux's advertised address (port {linux_port}), got {:?}",
            device_last_addr(&android, linux_id.clone()).unwrap()
        )
    });

    // ── And browsing from android must list linux's share ──
    let bytes = recorded.as_bytes();
    let (host, port) = match bytes.iter().rposition(|&b| b == b':') {
        Some(idx) => (
            String::from_utf8_lossy(&bytes[..idx]).to_string(),
            String::from_utf8_lossy(&bytes[idx + 1..])
                .parse::<u16>()
                .expect("port parses"),
        ),
        None => panic!("stored address has no port: {recorded}"),
    };
    let remote = browse_peer_shared_folders(&android, host, port)
        .await
        .unwrap();
    let shared = remote
        .iter()
        .find(|f| f.folder_guid == share_guid)
        .expect("linux's share should appear in android's remote folders");
    assert_eq!(shared.name, "shared-notes");

    stop_server(&android).await.ok();
    stop_server(&linux).await.ok();
}

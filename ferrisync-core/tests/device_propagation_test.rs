//! Propagation of trusted-device connections across peers.

use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::server::{serve_folder, PairPolicy};
use ferrisync_core::sync_engine::session;
use ferrisync_core::DeviceInfo;
use std::sync::Arc;
use tokio::sync::mpsc;

fn get_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    std::thread::sleep(std::time::Duration::from_millis(50));
    port
}

async fn device_info(crypto: &Arc<CryptoProvider>, name: &str) -> DeviceInfo {
    DeviceInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        cert_fingerprint: crypto.fingerprint().await,
    }
}

#[tokio::test]
async fn propagated_connections_are_visible_on_peer() {
    // ── Device A serves a folder ──
    let a_folder = tempfile::tempdir().unwrap();
    let a_data = tempfile::tempdir().unwrap();
    let port_a = get_available_port();

    let storage_a = Arc::new(Storage::open(&a_data.path().join("metadata.db")).unwrap());
    let crypto_a = Arc::new(CryptoProvider::generate().unwrap());
    let info_a = device_info(&crypto_a, "device-a").await;
    let (server_a, _events_a) = serve_folder(
        storage_a.clone(),
        crypto_a.clone(),
        info_a.clone(),
        a_folder.path().to_str().unwrap().to_string(),
        port_a,
        PairPolicy::AutoAccept,
        Arc::new(ferrisync_core::persistence::InMemoryStateStore::new()),
    )
    .await
    .unwrap();

    // ── Device B pairs with A ──
    let b_data = tempfile::tempdir().unwrap();
    let storage_b = Arc::new(Storage::open(&b_data.path().join("metadata.db")).unwrap());
    let crypto_b = Arc::new(CryptoProvider::generate().unwrap());
    let info_b = device_info(&crypto_b, "device-b").await;
    let pairing_b = PairingManager::new(crypto_b.clone(), storage_b.clone(), info_b.clone());
    let _peer_a_on_b = pairing_b
        .pair_with(format!("127.0.0.1:{port_a}").parse().unwrap())
        .await
        .unwrap();

    // ── Device C pairs with A ──
    let c_data = tempfile::tempdir().unwrap();
    let storage_c = Arc::new(Storage::open(&c_data.path().join("metadata.db")).unwrap());
    let crypto_c = Arc::new(CryptoProvider::generate().unwrap());
    let info_c = device_info(&crypto_c, "device-c").await;
    let pairing_c = PairingManager::new(crypto_c.clone(), storage_c.clone(), info_c.clone());
    let _peer_a_on_c = pairing_c
        .pair_with(format!("127.0.0.1:{port_a}").parse().unwrap())
        .await
        .unwrap();

    // ── Device B serves a folder ──
    let b_folder = tempfile::tempdir().unwrap();
    let port_b = get_available_port();
    let (server_b, _events_b) = serve_folder(
        storage_b.clone(),
        crypto_b.clone(),
        info_b.clone(),
        b_folder.path().to_str().unwrap().to_string(),
        port_b,
        PairPolicy::AutoAccept,
        Arc::new(ferrisync_core::persistence::InMemoryStateStore::new()),
    )
    .await
    .unwrap();

    // ── Device C pairs with B ──
    let pairing_c2 = PairingManager::new(crypto_c.clone(), storage_c.clone(), info_c.clone());
    let peer_b_on_c = pairing_c2
        .pair_with(format!("127.0.0.1:{port_b}").parse().unwrap())
        .await
        .unwrap();

    // ── C syncs with B, triggering propagation of B's connections (A) ──
    let folder_id = storage_c
        .add_sync_folder(
            c_data.path().to_str().unwrap(),
            &peer_b_on_c.id,
            "bidirectional",
        )
        .unwrap();
    // Use B's served folder path as the sync root on C's side? The session
    // needs a local folder; reuse C's data dir as an empty root.
    let c_sync_root = c_data.path().join("sync-root");
    std::fs::create_dir_all(&c_sync_root).unwrap();
    let (tx, _rx) = mpsc::channel(256);
    let result = session::run_sync_session(
        crypto_c.clone(),
        storage_c.clone(),
        c_sync_root.to_str().unwrap(),
        format!("127.0.0.1:{port_b}").parse().unwrap(),
        folder_id,
        &peer_b_on_c.id,
        tx,
        Arc::new(ferrisync_core::persistence::InMemoryStateStore::new()),
        false,
        None,
    )
    .await;
    assert!(result.is_ok(), "sync failed: {result:?}");

    // A's cert-derived id should now be visible on C.
    let a_id = ferrisync_core::crypto::cert_to_device_id(crypto_a.certificate().await.as_ref());
    let c_devices = storage_c.list_devices().unwrap();
    assert!(
        c_devices.iter().any(|(id, _, _)| id == &a_id),
        "device A was not propagated to C; got {c_devices:?}"
    );

    server_a.stop().await;
    server_b.stop().await;
}

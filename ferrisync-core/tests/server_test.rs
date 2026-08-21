use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::server::{serve_folder, PairPolicy};
use ferrisync_core::sync_engine::session;
use ferrisync_core::DeviceInfo;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

fn get_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    std::thread::sleep(Duration::from_millis(50));
    port
}

async fn device_info(crypto: &Arc<CryptoProvider>, name: &str) -> DeviceInfo {
    DeviceInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        cert_fingerprint: crypto.fingerprint().await,
    }
}

/// Pairing against an unreachable peer must fail within the transport
/// timeout (~5s), not hang for the kernel's default TCP timeout (~2min).
#[tokio::test]
async fn pair_to_unreachable_peer_fails_fast() {
    let data = tempfile::tempdir().unwrap();
    let storage = Arc::new(Storage::open(&data.path().join("metadata.db")).unwrap());
    let crypto = Arc::new(CryptoProvider::generate().unwrap());
    let pairing = PairingManager::new(
        crypto.clone(),
        storage,
        device_info(&crypto, "test-client").await,
    );

    let start = std::time::Instant::now();
    let result = pairing.pair_with("192.0.2.1:9847".parse().unwrap()).await;
    let elapsed = start.elapsed();

    assert!(result.is_err(), "pairing to unreachable peer must fail");
    assert!(
        elapsed < Duration::from_secs(15),
        "expected fast failure, took {elapsed:?}"
    );
}

/// In-process server: pair, sync a file, then verify the shutdown fix —
/// after `ServeHandle::stop` the port must refuse new connections.
#[tokio::test]
async fn serve_folder_accepts_sync_and_stops() {
    let server_folder = tempfile::tempdir().unwrap();
    let client_folder = tempfile::tempdir().unwrap();
    let server_data = tempfile::tempdir().unwrap();
    let client_data = tempfile::tempdir().unwrap();

    let port = get_available_port();

    // ── Server ──
    let storage_srv = Arc::new(Storage::open(&server_data.path().join("metadata.db")).unwrap());
    let crypto_srv = Arc::new(CryptoProvider::generate().unwrap());
    let info_srv = device_info(&crypto_srv, "test-server").await;
    let (server, mut events) = serve_folder(
        storage_srv.clone(),
        crypto_srv.clone(),
        info_srv.clone(),
        server_folder.path().to_str().unwrap().to_string(),
        port,
        PairPolicy::AutoAccept,
    )
    .await
    .unwrap();
    assert_eq!(server.port, port);

    // ── Client: pair, register folder, push a file ──
    let storage_cli = Arc::new(Storage::open(&client_data.path().join("metadata.db")).unwrap());
    let crypto_cli = Arc::new(CryptoProvider::generate().unwrap());
    let info_cli = device_info(&crypto_cli, "test-client").await;
    let pairing = PairingManager::new(crypto_cli.clone(), storage_cli.clone(), info_cli.clone());
    let peer = pairing
        .pair_with(format!("127.0.0.1:{port}").parse().unwrap())
        .await
        .unwrap();
    assert_eq!(peer.name, "test-server");

    // Pairing must have registered the client on the server side.
    let server_devices = storage_srv.list_devices().unwrap();
    assert!(server_devices.iter().any(|(id, _, _)| *id == info_cli.id));

    let local = client_folder.path().join("hello.txt");
    std::fs::write(&local, b"hello from client").unwrap();

    let folder_id = storage_cli
        .add_sync_folder(
            client_folder.path().to_str().unwrap(),
            &peer.id,
            "bidirectional",
        )
        .unwrap();
    let (event_tx, _event_rx) = mpsc::channel(256);
    let result = session::run_sync_session(
        crypto_cli,
        storage_cli.clone(),
        client_folder.path().to_str().unwrap(),
        format!("127.0.0.1:{port}").parse().unwrap(),
        folder_id,
        &peer.id,
        event_tx,
    )
    .await
    .unwrap();
    assert_eq!(result.pushed.len(), 1);

    // The file must land in the served folder (server writes asynchronously).
    let served = server_folder.path().join("hello.txt");
    let mut served_content = None;
    for _ in 0..50 {
        if let Ok(data) = std::fs::read(&served) {
            served_content = Some(data);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        served_content.expect("file never arrived"),
        b"hello from client"
    );

    // The server must announce pairing, then emit an event for the file.
    let mut file_event = None;
    for _ in 0..5 {
        let ev = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .expect("server should emit events");
        match ev {
            ferrisync_core::sync_engine::SyncEvent::FilePulled { path, .. } => {
                assert_eq!(path, "hello.txt");
                file_event = Some(path);
                break;
            }
            ferrisync_core::sync_engine::SyncEvent::DevicePaired { name, .. } => {
                assert_eq!(name, "test-client");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
    assert!(file_event.is_some(), "no file transfer event received");

    // ── Shutdown: the port must stop accepting connections ──
    server.stop().await;

    match tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await {
        Err(_) => {} // expected: nothing is listening anymore
        Ok(_) => panic!("port {port} still accepting connections after stop()"),
    }
}

/// Confirm policy: an unknown device is held (`PairRequested` event), pairing
/// succeeds once approved, and a known device pairs instantly afterwards.
#[tokio::test]
async fn confirm_policy_hold_then_approve() {
    let server_data = tempfile::tempdir().unwrap();
    let client_data = tempfile::tempdir().unwrap();
    let port = get_available_port();

    let storage_srv = Arc::new(Storage::open(&server_data.path().join("metadata.db")).unwrap());
    let crypto_srv = Arc::new(CryptoProvider::generate().unwrap());
    let info_srv = device_info(&crypto_srv, "test-server").await;
    let server_id = info_srv.id.clone();
    let (server, mut events) = serve_folder(
        storage_srv,
        crypto_srv,
        info_srv,
        "/tmp".to_string(),
        port,
        PairPolicy::Confirm,
    )
    .await
    .unwrap();

    // Client requests pairing in the background; it must be held first.
    let storage_cli = Arc::new(Storage::open(&client_data.path().join("metadata.db")).unwrap());
    let crypto_cli = Arc::new(CryptoProvider::generate().unwrap());
    let info_cli = device_info(&crypto_cli, "test-client").await;
    let client_id = info_cli.id.clone();
    let pairing = PairingManager::new(crypto_cli.clone(), storage_cli, info_cli);
    let task = tokio::spawn(async move {
        pairing
            .pair_with(format!("127.0.0.1:{port}").parse().unwrap())
            .await
    });

    match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
        Ok(Some(ferrisync_core::sync_engine::SyncEvent::PairRequested { name, id })) => {
            assert_eq!(name, "test-client");
            assert_eq!(id, client_id);
            assert!(
                server
                    .pending_pairings()
                    .unwrap()
                    .iter()
                    .any(|(_, pid)| *pid == id),
                "request must appear in pending list"
            );
            server.approve_pairing(&id, &name).unwrap();
        }
        other => panic!("expected PairRequested event, got {other:?}"),
    }

    // The retry loop picks up the approval and completes. The returned peer
    // info describes the SERVER (name + id as stored on its side).
    let peer = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(peer.name, "test-server");
    assert_eq!(peer.id, server_id);

    // The successful pairing is announced.
    match tokio::time::timeout(Duration::from_secs(2), events.recv()).await {
        Ok(Some(ferrisync_core::sync_engine::SyncEvent::DevicePaired { name, .. })) => {
            assert_eq!(name, "test-client");
        }
        other => panic!("expected DevicePaired event, got {other:?}"),
    }

    // Now that the device is known, a second pairing attempt is instant.
    let storage_cli2 = Arc::new(Storage::open(&client_data.path().join("m2.db")).unwrap());
    let crypto_cli2 = Arc::new(CryptoProvider::generate().unwrap());
    let info2 = ferrisync_core::DeviceInfo {
        id: client_id.clone(),
        name: "test-client".to_string(),
        cert_fingerprint: crypto_cli2.fingerprint().await,
    };
    let pairing2 = PairingManager::new(crypto_cli2, storage_cli2, info2);
    let start = std::time::Instant::now();
    pairing2
        .pair_with(format!("127.0.0.1:{port}").parse().unwrap())
        .await
        .unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "known device must pair instantly"
    );

    server.stop().await;
}

/// Confirm policy: denying a held request fails future attempts immediately.
#[tokio::test]
async fn confirm_policy_deny_rejects_client() {
    let server_data = tempfile::tempdir().unwrap();
    let client_data = tempfile::tempdir().unwrap();
    let port = get_available_port();

    let storage_srv = Arc::new(Storage::open(&server_data.path().join("metadata.db")).unwrap());
    let crypto_srv = Arc::new(CryptoProvider::generate().unwrap());
    let info_srv = device_info(&crypto_srv, "test-server").await;
    let (server, mut events) = serve_folder(
        storage_srv,
        crypto_srv,
        info_srv,
        "/tmp".to_string(),
        port,
        PairPolicy::Confirm,
    )
    .await
    .unwrap();

    let storage_cli = Arc::new(Storage::open(&client_data.path().join("metadata.db")).unwrap());
    let crypto_cli = Arc::new(CryptoProvider::generate().unwrap());
    let info_cli = device_info(&crypto_cli, "sneaky-client").await;
    let pairing = PairingManager::new(crypto_cli.clone(), storage_cli, info_cli);
    let task = tokio::spawn(async move {
        pairing
            .pair_with(format!("127.0.0.1:{port}").parse().unwrap())
            .await
    });

    let ev = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .expect("PairRequested event");
    let ferrisync_core::sync_engine::SyncEvent::PairRequested { id, .. } = ev else {
        panic!("unexpected event: {ev:?}");
    };
    server.deny_pairing(&id).unwrap();

    let result = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    let err = result.expect_err("denied pairing must fail");
    assert!(err.to_string().contains("denied"), "got: {err:#}");

    server.stop().await;
}

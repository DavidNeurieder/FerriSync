use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::server::serve_folder;
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

    // The server must have emitted an event for the received file.
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .expect("server should emit an event for the transferred file");
    match event {
        ferrisync_core::sync_engine::SyncEvent::FilePulled { path, .. } => {
            assert_eq!(path, "hello.txt");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    // ── Shutdown: the port must stop accepting connections ──
    server.stop().await;

    match tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await {
        Err(_) => {} // expected: nothing is listening anymore
        Ok(_) => panic!("port {port} still accepting connections after stop()"),
    }
}

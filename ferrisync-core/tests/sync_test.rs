use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Test: basic file sync between two peers
#[tokio::test]
async fn test_basic_sync() {
    // Create temp directories for two peers
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    // Create a test file in A
    let test_file = dir_a.path().join("hello.txt");
    std::fs::write(&test_file, b"Hello from peer A").unwrap();

    // Set up crypto and storage for both peers
    let crypto_a = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_b = Arc::new(CryptoProvider::generate().unwrap());

    let storage_a = Arc::new(
        Storage::open(&dir_a.path().join("metadata.db")).unwrap(),
    );
    let storage_b = Arc::new(
        Storage::open(&dir_b.path().join("metadata.db")).unwrap(),
    );

    let (tx_a, _rx_a) = mpsc::channel(256);
    let (_tx_b, _rx_b) = mpsc::channel(256);

    let dev_a_id = uuid::Uuid::new_v4();
    let dev_b_id = uuid::Uuid::new_v4();

    storage_a
        .upsert_device(&dev_b_id.to_string(), "peer_b", None)
        .unwrap();
    storage_b
        .upsert_device(&dev_a_id.to_string(), "peer_a", None)
        .unwrap();

    let _folder_id_a = storage_a
        .add_sync_folder(dir_a.path().to_str().unwrap(), &dev_b_id.to_string(), "bidirectional")
        .unwrap();
    let folder_id_b = storage_b
        .add_sync_folder(dir_b.path().to_str().unwrap(), &dev_a_id.to_string(), "bidirectional")
        .unwrap();

    // Start peer B listening
    let listen_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(listen_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let crypto_b_listen = crypto_b.clone();
    let storage_b_listen = storage_b.clone();
    let path_b = dir_b.path().to_str().unwrap().to_string();
    let tx_b_clone = _tx_b.clone();

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let crypto = crypto_b_listen.clone();
                    let storage = storage_b_listen.clone();
                    let local_path = path_b.clone();
                    let event_tx = tx_b_clone.clone();

                    tokio::spawn(async move {
                        let config = crypto.server_config().await.unwrap();
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
                        let mut tls = match acceptor.accept(tcp).await {
                            Ok(t) => tokio_rustls::TlsStream::Server(t),
                            Err(e) => {
                                eprintln!("TLS accept failed: {e}");
                                return;
                            }
                        };

                        let _ = session::handle_server_session(
                            &mut tls,
                            crypto,
                            storage,
                            &local_path,
                            folder_id_b,
                            event_tx,
                        )
                        .await;
                    });
                }
                Err(e) => {
                    eprintln!("accept error: {e}");
                }
            }
        }
    });

    // Small delay to ensure listener is ready
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Peer A initiates sync to peer B
    let _result = session::run_sync_session(
        crypto_a.clone(),
        storage_a.clone(),
        dir_a.path().to_str().unwrap(),
        actual_addr,
        _folder_id_a,
        &dev_b_id.to_string(),
        tx_a.clone(),
    )
    .await
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let b_file = dir_b.path().join("hello.txt");
    assert!(b_file.exists(), "hello.txt should exist on peer B");
    let content = std::fs::read_to_string(&b_file).unwrap();
    assert_eq!(content, "Hello from peer A");
}

/// Test: sync is bidirectional
#[tokio::test]
async fn test_bidirectional_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    // Each peer has a unique file
    std::fs::write(dir_a.path().join("from_a.txt"), b"File from A").unwrap();
    std::fs::write(dir_b.path().join("from_b.txt"), b"File from B").unwrap();

    let crypto_a = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_b = Arc::new(CryptoProvider::generate().unwrap());

    let storage_a = Arc::new(Storage::open(&dir_a.path().join("metadata.db")).unwrap());
    let storage_b = Arc::new(Storage::open(&dir_b.path().join("metadata.db")).unwrap());

    let (tx_a, _rx_a) = mpsc::channel(256);

    let dev_a_id = uuid::Uuid::new_v4();
    let dev_b_id = uuid::Uuid::new_v4();

    storage_a
        .upsert_device(&dev_b_id.to_string(), "peer_b", None)
        .unwrap();
    storage_b
        .upsert_device(&dev_a_id.to_string(), "peer_a", None)
        .unwrap();

    let folder_id_a = storage_a
        .add_sync_folder(dir_a.path().to_str().unwrap(), &dev_b_id.to_string(), "bidirectional")
        .unwrap();
    let folder_id_b = storage_b
        .add_sync_folder(dir_b.path().to_str().unwrap(), &dev_a_id.to_string(), "bidirectional")
        .unwrap();

    // Start B as server
    let listen_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(listen_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let crypto_b_srv = crypto_b.clone();
    let storage_b_srv = storage_b.clone();
    let path_b = dir_b.path().to_str().unwrap().to_string();
    let (tx_b, _rx_b) = mpsc::channel(256);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let crypto = crypto_b_srv.clone();
                    let storage = storage_b_srv.clone();
                    let local_path = path_b.clone();
                    let event_tx = tx_b.clone();

                    tokio::spawn(async move {
                        let config = crypto.server_config().await.unwrap();
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
                        let tls = acceptor.accept(tcp).await.unwrap();
                        let _ = session::handle_server_session(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            crypto,
                            storage,
                            &local_path,
                            folder_id_b,
                            event_tx,
                        )
                        .await;
                    });
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // A initiates sync with B
    let result = session::run_sync_session(
        crypto_a.clone(),
        storage_a.clone(),
        dir_a.path().to_str().unwrap(),
        actual_addr,
        folder_id_a,
        &dev_b_id.to_string(),
        tx_a.clone(),
    )
    .await
    .unwrap();

    // A should have pushed from_a.txt to B
    assert!(result.pushed.contains(&"from_a.txt".to_string()));

    // Give the server a moment to process and write the file
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify both files exist on both peers
    assert!(dir_a.path().join("from_b.txt").exists(), "from_b.txt should exist on peer A");
    assert!(dir_b.path().join("from_a.txt").exists(), "from_a.txt should exist on peer B");
}

use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use ferrisync_core::sync_engine::SyncEvent;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Test: basic file sync between two peers
#[tokio::test]
async fn test_basic_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let test_file = dir_a.path().join("hello.txt");
    std::fs::write(&test_file, b"Hello from peer A").unwrap();

    let crypto_a = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_b = Arc::new(CryptoProvider::generate().unwrap());

    let storage_a = Arc::new(Storage::open(&dir_a.path().join("metadata.db")).unwrap());
    let storage_b = Arc::new(Storage::open(&dir_b.path().join("metadata.db")).unwrap());

    let (tx_a, _rx_a) = mpsc::channel(256);
    let (_tx_b, _rx_b) = mpsc::channel(256);

    let dev_a_id = uuid::Uuid::new_v4();
    let dev_b_id = uuid::Uuid::new_v4();

    storage_a
        .upsert_device(&dev_b_id.to_string(), "peer_b", None, None)
        .unwrap();
    storage_b
        .upsert_device(&dev_a_id.to_string(), "peer_a", None, None)
        .unwrap();

    let _folder_id_a = storage_a
        .add_sync_folder(
            dir_a.path().to_str().unwrap(),
            &dev_b_id.to_string(),
            "bidirectional",
        )
        .unwrap();
    let folder_id_b = storage_b
        .add_sync_folder(
            dir_b.path().to_str().unwrap(),
            &dev_a_id.to_string(),
            "bidirectional",
        )
        .unwrap();

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

                        let _ = session::handle_server_session_with_read(
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

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

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
        .upsert_device(&dev_b_id.to_string(), "peer_b", None, None)
        .unwrap();
    storage_b
        .upsert_device(&dev_a_id.to_string(), "peer_a", None, None)
        .unwrap();

    let folder_id_a = storage_a
        .add_sync_folder(
            dir_a.path().to_str().unwrap(),
            &dev_b_id.to_string(),
            "bidirectional",
        )
        .unwrap();
    let folder_id_b = storage_b
        .add_sync_folder(
            dir_b.path().to_str().unwrap(),
            &dev_a_id.to_string(),
            "bidirectional",
        )
        .unwrap();

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
                        let _ = session::handle_server_session_with_read(
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

    assert!(result.pushed.contains(&"from_a.txt".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert!(
        dir_a.path().join("from_b.txt").exists(),
        "from_b.txt should exist on peer A"
    );
    assert!(
        dir_b.path().join("from_a.txt").exists(),
        "from_a.txt should exist on peer B"
    );
}

/// Test: multiple files and nested directories sync with session code path
#[tokio::test]
async fn test_flutter_sync_roundtrip() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    // A has: root.txt, subdir/nested.txt
    // B has: other.txt
    std::fs::create_dir_all(dir_a.path().join("subdir")).unwrap();
    std::fs::write(dir_a.path().join("root.txt"), b"Root file from A").unwrap();
    std::fs::write(
        dir_a.path().join("subdir").join("nested.txt"),
        b"Nested file from A",
    )
    .unwrap();
    std::fs::write(dir_b.path().join("other.txt"), b"Other file from B").unwrap();

    let crypto_a = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_b = Arc::new(CryptoProvider::generate().unwrap());

    let storage_a = Arc::new(Storage::open(&dir_a.path().join("metadata.db")).unwrap());
    let storage_b = Arc::new(Storage::open(&dir_b.path().join("metadata.db")).unwrap());

    let (tx_a, _rx_a) = mpsc::channel(256);

    let dev_a_id = uuid::Uuid::new_v4();
    let dev_b_id = uuid::Uuid::new_v4();

    storage_a
        .upsert_device(&dev_b_id.to_string(), "peer_b", None, None)
        .unwrap();
    storage_b
        .upsert_device(&dev_a_id.to_string(), "peer_a", None, None)
        .unwrap();

    let folder_id_a = storage_a
        .add_sync_folder(
            dir_a.path().to_str().unwrap(),
            &dev_b_id.to_string(),
            "bidirectional",
        )
        .unwrap();
    let folder_id_b = storage_b
        .add_sync_folder(
            dir_b.path().to_str().unwrap(),
            &dev_a_id.to_string(),
            "bidirectional",
        )
        .unwrap();

    // Server B
    let listen_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(listen_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cb = crypto_b.clone();
    let sb = storage_b.clone();
    let pb = dir_b.path().to_str().unwrap().to_string();
    let (tx_b, _rx_b) = mpsc::channel(256);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let c = cb.clone();
                    let s = sb.clone();
                    let p = pb.clone();
                    let ev = tx_b.clone();
                    tokio::spawn(async move {
                        let config = c.server_config().await.unwrap();
                        let a = tokio_rustls::TlsAcceptor::from(config);
                        let tls = a.accept(tcp).await.unwrap();
                        let _ = session::handle_server_session_with_read(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            c,
                            s,
                            &p,
                            folder_id_b,
                            ev,
                        )
                        .await;
                    });
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

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

    // A should have pushed root.txt and subdir/nested.txt to B
    assert!(
        result.pushed.contains(&"root.txt".to_string()),
        "root.txt should be pushed"
    );
    assert!(
        result.pushed.contains(&"subdir/nested.txt".to_string()),
        "subdir/nested.txt should be pushed"
    );
    assert!(
        result.pulled.contains(&"other.txt".to_string()),
        "other.txt should be pulled"
    );

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Verify A got B's files
    assert!(
        dir_a.path().join("other.txt").exists(),
        "other.txt should exist on A"
    );
    assert_eq!(
        std::fs::read_to_string(dir_a.path().join("other.txt")).unwrap(),
        "Other file from B"
    );

    // Verify B got A's files
    assert!(
        dir_b.path().join("root.txt").exists(),
        "root.txt should exist on B"
    );
    assert_eq!(
        std::fs::read_to_string(dir_b.path().join("root.txt")).unwrap(),
        "Root file from A"
    );
    assert!(
        dir_b.path().join("subdir").join("nested.txt").exists(),
        "nested.txt should exist on B"
    );
    assert_eq!(
        std::fs::read_to_string(dir_b.path().join("subdir").join("nested.txt")).unwrap(),
        "Nested file from A"
    );
}

/// Test: session path pulls file from server (replaces old CLI code path test)
#[tokio::test]
async fn test_cli_code_path_sync() {
    let dir_client = tempfile::tempdir().unwrap();
    let dir_server = tempfile::tempdir().unwrap();

    std::fs::write(
        dir_server.path().join("server_file.txt"),
        b"Hello from server",
    )
    .unwrap();

    let crypto_server = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_client = Arc::new(CryptoProvider::generate().unwrap());

    let storage_client = Arc::new(Storage::open(&dir_client.path().join("metadata.db")).unwrap());
    let storage_server = Arc::new(Storage::open(&dir_server.path().join("metadata.db")).unwrap());

    let dev_id = uuid::Uuid::new_v4().to_string();
    let client_id = uuid::Uuid::new_v4().to_string();
    storage_client
        .upsert_device(&dev_id, "server", None, None)
        .unwrap();
    storage_server
        .upsert_device(&client_id, "client", None, None)
        .unwrap();

    let folder_id_client = storage_client
        .add_sync_folder(
            dir_client.path().to_str().unwrap(),
            &dev_id,
            "bidirectional",
        )
        .unwrap();
    let folder_id_server = storage_server
        .add_sync_folder(
            dir_server.path().to_str().unwrap(),
            &client_id,
            "bidirectional",
        )
        .unwrap();

    let server_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cs = crypto_server.clone();
    let ss = storage_server.clone();
    let ps = dir_server.path().to_str().unwrap().to_string();
    let (tx_s, _rx_s) = mpsc::channel(256);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let c = cs.clone();
                    let s = ss.clone();
                    let p = ps.clone();
                    let ev = tx_s.clone();
                    tokio::spawn(async move {
                        let config = c.server_config().await.unwrap();
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
                        let tls = acceptor.accept(tcp).await.unwrap();
                        let _ = session::handle_server_session_with_read(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            c,
                            s,
                            &p,
                            folder_id_server,
                            ev,
                        )
                        .await;
                    });
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx_c, _rx_c) = mpsc::channel(256);
    let result = session::run_sync_session(
        crypto_client.clone(),
        storage_client.clone(),
        dir_client.path().to_str().unwrap(),
        actual_addr,
        folder_id_client,
        &dev_id,
        tx_c.clone(),
    )
    .await;

    assert!(result.is_ok(), "sync should succeed: {:?}", result.err());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let client_file = dir_client.path().join("server_file.txt");
    assert!(
        client_file.exists(),
        "server_file.txt should exist on client"
    );
    let content = std::fs::read_to_string(&client_file).unwrap();
    assert_eq!(content, "Hello from server");
}

/// Test: session path conflict resolution via mtime (newer wins)
#[tokio::test]
async fn test_cli_code_path_conflict_resolution() {
    let dir_client = tempfile::tempdir().unwrap();
    let dir_server = tempfile::tempdir().unwrap();

    std::fs::write(
        dir_client.path().join("shared.txt"),
        b"Client version (older)",
    )
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    std::fs::write(dir_server.path().join("shared.txt"), b"Server version").unwrap();

    let crypto_server = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_client = Arc::new(CryptoProvider::generate().unwrap());

    let storage_client = Arc::new(Storage::open(&dir_client.path().join("metadata.db")).unwrap());
    let storage_server = Arc::new(Storage::open(&dir_server.path().join("metadata.db")).unwrap());

    let dev_id = uuid::Uuid::new_v4().to_string();
    let client_id = uuid::Uuid::new_v4().to_string();
    storage_client
        .upsert_device(&dev_id, "server", None, None)
        .unwrap();
    storage_server
        .upsert_device(&client_id, "client", None, None)
        .unwrap();

    let folder_id_client = storage_client
        .add_sync_folder(
            dir_client.path().to_str().unwrap(),
            &dev_id,
            "bidirectional",
        )
        .unwrap();
    let folder_id_server = storage_server
        .add_sync_folder(
            dir_server.path().to_str().unwrap(),
            &client_id,
            "bidirectional",
        )
        .unwrap();

    let server_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cs = crypto_server.clone();
    let ss = storage_server.clone();
    let ps = dir_server.path().to_str().unwrap().to_string();
    let (tx_s, _rx_s) = mpsc::channel(256);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let c = cs.clone();
                    let s = ss.clone();
                    let p = ps.clone();
                    let ev = tx_s.clone();
                    tokio::spawn(async move {
                        let config = c.server_config().await.unwrap();
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
                        let tls = acceptor.accept(tcp).await.unwrap();
                        let _ = session::handle_server_session_with_read(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            c,
                            s,
                            &p,
                            folder_id_server,
                            ev,
                        )
                        .await;
                    });
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx_c, _rx_c) = mpsc::channel(256);
    let result = session::run_sync_session(
        crypto_client.clone(),
        storage_client.clone(),
        dir_client.path().to_str().unwrap(),
        actual_addr,
        folder_id_client,
        &dev_id,
        tx_c.clone(),
    )
    .await;

    assert!(result.is_ok(), "sync should succeed: {:?}", result.err());
    let result = result.unwrap();
    assert_eq!(
        result.conflicts,
        vec!["shared.txt"],
        "conflict should be detected and recorded"
    );
    assert!(
        result.pulled.contains(&"shared.txt".to_string()),
        "file should be pulled"
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify the winning version
    let shared_file = dir_client.path().join("shared.txt");
    let content = std::fs::read_to_string(&shared_file).unwrap();
    assert_eq!(content, "Server version", "newer mtime should win");

    // Verify the backup of the losing version
    let bak_file = dir_client.path().join("shared.txt.bak");
    assert!(bak_file.exists(), "conflict backup should exist");
    let bak_content = std::fs::read_to_string(&bak_file).unwrap();
    assert_eq!(
        bak_content, "Client version (older)",
        "backup should contain the losing version"
    );
}

/// Test: empty sync (no files on either side) via session path
#[tokio::test]
async fn test_cli_code_path_empty_sync() {
    let dir_client = tempfile::tempdir().unwrap();
    let dir_server = tempfile::tempdir().unwrap();

    let crypto_server = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_client = Arc::new(CryptoProvider::generate().unwrap());

    let storage_client = Arc::new(Storage::open(&dir_client.path().join("metadata.db")).unwrap());
    let storage_server = Arc::new(Storage::open(&dir_server.path().join("metadata.db")).unwrap());

    let dev_id = uuid::Uuid::new_v4().to_string();
    let client_id = uuid::Uuid::new_v4().to_string();
    storage_client
        .upsert_device(&dev_id, "server", None, None)
        .unwrap();
    storage_server
        .upsert_device(&client_id, "client", None, None)
        .unwrap();

    let folder_id_client = storage_client
        .add_sync_folder(
            dir_client.path().to_str().unwrap(),
            &dev_id,
            "bidirectional",
        )
        .unwrap();
    let folder_id_server = storage_server
        .add_sync_folder(
            dir_server.path().to_str().unwrap(),
            &client_id,
            "bidirectional",
        )
        .unwrap();

    let server_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cs = crypto_server.clone();
    let ss = storage_server.clone();
    let ps = dir_server.path().to_str().unwrap().to_string();
    let (tx_s, _rx_s) = mpsc::channel(256);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let c = cs.clone();
                    let s = ss.clone();
                    let p = ps.clone();
                    let ev = tx_s.clone();
                    tokio::spawn(async move {
                        let config = c.server_config().await.unwrap();
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
                        let tls = acceptor.accept(tcp).await.unwrap();
                        let _ = session::handle_server_session_with_read(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            c,
                            s,
                            &p,
                            folder_id_server,
                            ev,
                        )
                        .await;
                    });
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx_c, _rx_c) = mpsc::channel(256);
    let result = session::run_sync_session(
        crypto_client.clone(),
        storage_client.clone(),
        dir_client.path().to_str().unwrap(),
        actual_addr,
        folder_id_client,
        &dev_id,
        tx_c.clone(),
    )
    .await;

    assert!(
        result.is_ok(),
        "empty sync should succeed: {:?}",
        result.err()
    );
}

/// Test: session path small file transfer (single chunk)
#[tokio::test]
async fn test_cli_code_path_small_file() {
    let dir_client = tempfile::tempdir().unwrap();
    let dir_server = tempfile::tempdir().unwrap();

    let data = b"Small file content that fits in one chunk";
    std::fs::write(dir_server.path().join("small.txt"), data).unwrap();

    let crypto_server = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_client = Arc::new(CryptoProvider::generate().unwrap());

    let storage_client = Arc::new(Storage::open(&dir_client.path().join("metadata.db")).unwrap());
    let storage_server = Arc::new(Storage::open(&dir_server.path().join("metadata.db")).unwrap());

    let dev_id = uuid::Uuid::new_v4().to_string();
    let client_id = uuid::Uuid::new_v4().to_string();
    storage_client
        .upsert_device(&dev_id, "server", None, None)
        .unwrap();
    storage_server
        .upsert_device(&client_id, "client", None, None)
        .unwrap();

    let folder_id_client = storage_client
        .add_sync_folder(
            dir_client.path().to_str().unwrap(),
            &dev_id,
            "bidirectional",
        )
        .unwrap();
    let folder_id_server = storage_server
        .add_sync_folder(
            dir_server.path().to_str().unwrap(),
            &client_id,
            "bidirectional",
        )
        .unwrap();

    let server_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cs = crypto_server.clone();
    let ss = storage_server.clone();
    let ps = dir_server.path().to_str().unwrap().to_string();
    let (tx_s, _rx_s) = mpsc::channel(256);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let c = cs.clone();
                    let s = ss.clone();
                    let p = ps.clone();
                    let ev = tx_s.clone();
                    tokio::spawn(async move {
                        let config = c.server_config().await.unwrap();
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
                        let tls = acceptor.accept(tcp).await.unwrap();
                        let _ = session::handle_server_session_with_read(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            c,
                            s,
                            &p,
                            folder_id_server,
                            ev,
                        )
                        .await;
                    });
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx_c, _rx_c) = mpsc::channel(256);
    let result = session::run_sync_session(
        crypto_client.clone(),
        storage_client.clone(),
        dir_client.path().to_str().unwrap(),
        actual_addr,
        folder_id_client,
        &dev_id,
        tx_c.clone(),
    )
    .await;

    assert!(
        result.is_ok(),
        "small file sync should succeed: {:?}",
        result.err()
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let client_file = dir_client.path().join("small.txt");
    assert!(client_file.exists(), "small.txt should exist on client");
    let content = std::fs::read(&client_file).unwrap();
    assert_eq!(content, data, "file content should match");
}

/// Test: session code path handles multi-chunk files correctly
#[tokio::test]
async fn test_flutter_sync_large_file() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    // Create a file larger than CHUNK_SIZE (64KB) on A
    let large_data = vec![b'B'; 300_000];
    std::fs::write(dir_a.path().join("large.bin"), &large_data).unwrap();

    let crypto_a = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_b = Arc::new(CryptoProvider::generate().unwrap());

    let storage_a = Arc::new(Storage::open(&dir_a.path().join("metadata.db")).unwrap());
    let storage_b = Arc::new(Storage::open(&dir_b.path().join("metadata.db")).unwrap());

    let (tx_a, _rx_a) = mpsc::channel(256);

    let dev_a_id = uuid::Uuid::new_v4();
    let dev_b_id = uuid::Uuid::new_v4();

    storage_a
        .upsert_device(&dev_b_id.to_string(), "peer_b", None, None)
        .unwrap();
    storage_b
        .upsert_device(&dev_a_id.to_string(), "peer_a", None, None)
        .unwrap();

    let folder_id_a = storage_a
        .add_sync_folder(
            dir_a.path().to_str().unwrap(),
            &dev_b_id.to_string(),
            "bidirectional",
        )
        .unwrap();
    let folder_id_b = storage_b
        .add_sync_folder(
            dir_b.path().to_str().unwrap(),
            &dev_a_id.to_string(),
            "bidirectional",
        )
        .unwrap();

    // Server B
    let listen_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(listen_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cb = crypto_b.clone();
    let sb = storage_b.clone();
    let pb = dir_b.path().to_str().unwrap().to_string();
    let (tx_b, _rx_b) = mpsc::channel(256);

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let c = cb.clone();
                    let s = sb.clone();
                    let p = pb.clone();
                    let ev = tx_b.clone();
                    tokio::spawn(async move {
                        let config = c.server_config().await.unwrap();
                        let a = tokio_rustls::TlsAcceptor::from(config);
                        let tls = a.accept(tcp).await.unwrap();
                        let _ = session::handle_server_session_with_read(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            c,
                            s,
                            &p,
                            folder_id_b,
                            ev,
                        )
                        .await;
                    });
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

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

    assert!(result.pushed.contains(&"large.bin".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify large file landed on B intact
    let b_file = dir_b.path().join("large.bin");
    assert!(b_file.exists(), "large.bin should exist on peer B");
    let content = std::fs::read(&b_file).unwrap();
    assert_eq!(content.len(), 300_000, "file size should match");
    assert_eq!(content, large_data, "file content should match");
}

/// Storage: removing sync entries by path, optionally narrowed to one device.
/// A path can legitimately map to several rows (one per device).
#[tokio::test]
async fn test_remove_sync_folders_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(&dir.path().join("metadata.db")).unwrap();

    storage.upsert_device("dev-a", "a", None, None).unwrap();
    storage.upsert_device("dev-b", "b", None, None).unwrap();
    storage
        .add_sync_folder("/tmp/shared", "dev-a", "bidirectional")
        .unwrap();
    storage
        .add_sync_folder("/tmp/shared", "dev-b", "bidirectional")
        .unwrap();
    storage
        .add_sync_folder("/tmp/other", "dev-a", "bidirectional")
        .unwrap();

    // Unknown device removes nothing.
    assert_eq!(
        storage
            .remove_sync_folders("/tmp/shared", Some("dev-x"))
            .unwrap(),
        0
    );

    // Device-scoped removal takes out exactly one row.
    assert_eq!(
        storage
            .remove_sync_folders("/tmp/shared", Some("dev-a"))
            .unwrap(),
        1
    );
    assert_eq!(storage.list_sync_folders().unwrap().len(), 2);

    // Unscoped removal clears the rest of that path only.
    assert_eq!(storage.remove_sync_folders("/tmp/shared", None).unwrap(), 1);
    let remaining = storage.list_sync_folders().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].1, "/tmp/other");

    // Removing a path with no rows reports zero, not an error.
    assert_eq!(storage.remove_sync_folders("/nope", None).unwrap(), 0);
}

/// Storage: full reset wipes folders, devices, and per-folder caches in one
/// call, and is a no-op on an already-empty database.
#[tokio::test]
async fn test_clear_all_sync_state() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());

    storage.upsert_device("dev-a", "a", None, None).unwrap();
    storage.upsert_device("dev-b", "b", None, None).unwrap();
    let f1 = storage
        .add_sync_folder("/tmp/one", "dev-a", "bidirectional")
        .unwrap();
    storage
        .add_sync_folder("/tmp/two", "dev-b", "bidirectional")
        .unwrap();
    storage
        .upsert_file_metadata(f1, "stale.txt", 1, 3, b"hash", "dev-a")
        .unwrap();

    let (folders, devices) = storage.clear_all_sync_state().unwrap();
    assert_eq!((folders, devices), (2, 2));
    assert!(storage.list_sync_folders().unwrap().is_empty());
    assert!(storage.list_devices().unwrap().is_empty());

    // Resetting again is fine and reports zeros.
    assert_eq!(storage.clear_all_sync_state().unwrap(), (0, 0));
}

struct TestSide {
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    dir: tempfile::TempDir,
    folder_id: i64,
}

impl TestSide {
    fn new(other_device_id: &str, other_name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
        storage
            .upsert_device(other_device_id, other_name, None, None)
            .unwrap();
        let folder_id = storage
            .add_sync_folder(
                dir.path().to_str().unwrap(),
                other_device_id,
                "bidirectional",
            )
            .unwrap();
        Self {
            crypto: Arc::new(CryptoProvider::generate().unwrap()),
            storage,
            dir,
            folder_id,
        }
    }

    fn path(&self) -> &str {
        self.dir.path().to_str().unwrap()
    }
}

async fn spawn_server(
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    local_path: String,
    folder_id: i64,
    event_tx: mpsc::Sender<SyncEvent>,
) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let crypto = crypto.clone();
            let storage = storage.clone();
            let local_path = local_path.clone();
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                let config = match crypto.server_config().await {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let acceptor = tokio_rustls::TlsAcceptor::from(config);
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let _ = session::handle_server_session_with_read(
                        &mut tokio_rustls::TlsStream::Server(tls),
                        crypto,
                        storage,
                        &local_path,
                        folder_id,
                        event_tx,
                    )
                    .await;
                }
            });
        }
    });
    addr
}

fn drain_file_events(rx: &mut mpsc::Receiver<SyncEvent>) -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        match ev {
            SyncEvent::FilePushed { path, .. } => out.push(format!("push:{path}")),
            SyncEvent::FilePulled { path, .. } => out.push(format!("pull:{path}")),
            _ => {}
        }
    }
    out.sort();
    out
}

#[tokio::test]
async fn test_incremental_sync_modification() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("f1.txt"), b"v1").unwrap();
    std::fs::write(b.dir.path().join("g1.txt"), b"g-v1").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx1, _rx1) = mpsc::channel(256);
    let r1 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx1,
    )
    .await
    .unwrap();
    assert!(r1.pushed.contains(&"f1.txt".to_string()));
    assert!(r1.pulled.contains(&"g1.txt".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        std::fs::read_to_string(b.dir.path().join("f1.txt")).unwrap(),
        "v1"
    );
    assert_eq!(
        std::fs::read_to_string(a.dir.path().join("g1.txt")).unwrap(),
        "g-v1"
    );

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    std::fs::write(a.dir.path().join("f1.txt"), b"v2-longer-content").unwrap();
    std::fs::write(a.dir.path().join("new.txt"), b"brand new").unwrap();
    std::fs::write(b.dir.path().join("g1.txt"), b"g-v2-modified").unwrap();

    let (tx2, mut rx2) = mpsc::channel(256);
    let r2 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx2,
    )
    .await
    .unwrap();
    assert!(r2.pushed.contains(&"f1.txt".to_string()));
    assert!(r2.pushed.contains(&"new.txt".to_string()));
    assert!(r2.pulled.contains(&"g1.txt".to_string()));
    assert_eq!(r2.conflicts, vec!["g1.txt".to_string()]);

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        std::fs::read_to_string(b.dir.path().join("f1.txt")).unwrap(),
        "v2-longer-content"
    );
    assert_eq!(
        std::fs::read_to_string(b.dir.path().join("new.txt")).unwrap(),
        "brand new"
    );
    assert_eq!(
        std::fs::read_to_string(a.dir.path().join("g1.txt")).unwrap(),
        "g-v2-modified"
    );

    let events = drain_file_events(&mut rx2);
    assert!(events.contains(&"push:f1.txt".to_string()));
    assert!(events.contains(&"pull:g1.txt".to_string()));
}

#[tokio::test]
async fn test_deep_nested_directories() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::create_dir_all(a.dir.path().join("deep/x/y")).unwrap();
    std::fs::write(a.dir.path().join("deep/x/y/z.txt"), b"deep-a").unwrap();
    std::fs::create_dir_all(b.dir.path().join("other/p/q")).unwrap();
    std::fs::write(b.dir.path().join("other/p/q/r.txt"), b"deep-b").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx, _rx) = mpsc::channel(256);
    session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx,
    )
    .await
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        std::fs::read_to_string(a.dir.path().join("other/p/q/r.txt")).unwrap(),
        "deep-b"
    );
    assert_eq!(
        std::fs::read_to_string(b.dir.path().join("deep/x/y/z.txt")).unwrap(),
        "deep-a"
    );
}

#[tokio::test]
async fn test_edge_case_files() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("empty.txt"), b"").unwrap();
    let mut binary: Vec<u8> = vec![0u8; 64];
    binary.extend(0u8..=255);
    std::fs::write(a.dir.path().join("bin.dat"), &binary).unwrap();
    std::fs::write(a.dir.path().join("héllo wörld 🎉.txt"), b"unicode content").unwrap();
    std::fs::write(a.dir.path().join(".hidden"), b"dotfile").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx, _rx) = mpsc::channel(256);
    let result = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx,
    )
    .await
    .unwrap();
    assert_eq!(result.pushed.len(), 4, "all four files pushed");

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(
        std::fs::read(b.dir.path().join("empty.txt")).unwrap(),
        Vec::<u8>::new()
    );
    assert_eq!(std::fs::read(b.dir.path().join("bin.dat")).unwrap(), binary);
    assert_eq!(
        std::fs::read_to_string(b.dir.path().join("héllo wörld 🎉.txt")).unwrap(),
        "unicode content"
    );
    assert_eq!(
        std::fs::read_to_string(b.dir.path().join(".hidden")).unwrap(),
        "dotfile"
    );
}

#[tokio::test]
async fn test_noop_sync_transfers_nothing() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("x.txt"), b"x").unwrap();
    std::fs::write(b.dir.path().join("y.txt"), b"y").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx1, _rx1) = mpsc::channel(256);
    let r1 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx1,
    )
    .await
    .unwrap();
    assert_eq!(r1.pushed.len() + r1.pulled.len(), 2);

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (tx2, mut rx2) = mpsc::channel(256);
    let r2 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx2,
    )
    .await
    .unwrap();
    assert!(r2.pushed.is_empty(), "nothing should push on second sync");
    assert!(r2.pulled.is_empty(), "nothing should pull on second sync");
    assert!(
        drain_file_events(&mut rx2).is_empty(),
        "no file events expected on no-op sync"
    );
}

#[tokio::test]
async fn test_sequential_distinct_clients() {
    let id_srv = uuid::Uuid::new_v4().to_string();
    let id_c1 = uuid::Uuid::new_v4().to_string();
    let id_c2 = uuid::Uuid::new_v4().to_string();

    let srv = TestSide::new(&id_c1, "client-1");
    srv.storage
        .upsert_device(&id_c2, "client-2", None, None)
        .unwrap();
    let c1 = TestSide::new(&id_srv, "server");
    let c2 = TestSide::new(&id_srv, "server");

    std::fs::write(srv.dir.path().join("shared.txt"), b"shared-data").unwrap();

    let (_tx_srv, _rx_srv) = mpsc::channel(256);
    let addr = spawn_server(
        srv.crypto.clone(),
        srv.storage.clone(),
        srv.path().to_string(),
        srv.folder_id,
        _tx_srv,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (t1, _r1) = mpsc::channel(256);
    let res1 = session::run_sync_session(
        c1.crypto.clone(),
        c1.storage.clone(),
        c1.path(),
        addr,
        c1.folder_id,
        &id_srv,
        t1,
    )
    .await
    .unwrap();
    assert!(res1.pulled.contains(&"shared.txt".to_string()));

    std::fs::write(c1.dir.path().join("a1.txt"), b"from-client-1").unwrap();
    let (t2, _r2) = mpsc::channel(256);
    let res2 = session::run_sync_session(
        c1.crypto.clone(),
        c1.storage.clone(),
        c1.path(),
        addr,
        c1.folder_id,
        &id_srv,
        t2,
    )
    .await
    .unwrap();
    assert!(res2.pushed.contains(&"a1.txt".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (t3, _r3) = mpsc::channel(256);
    let res3 = session::run_sync_session(
        c2.crypto.clone(),
        c2.storage.clone(),
        c2.path(),
        addr,
        c2.folder_id,
        &id_srv,
        t3,
    )
    .await
    .unwrap();
    assert!(res3.pulled.contains(&"shared.txt".to_string()));
    assert!(res3.pulled.contains(&"a1.txt".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    std::fs::write(c2.dir.path().join("a2.txt"), b"from-client-2").unwrap();

    let (t4, _r4) = mpsc::channel(256);
    let res4 = session::run_sync_session(
        c2.crypto.clone(),
        c2.storage.clone(),
        c2.path(),
        addr,
        c2.folder_id,
        &id_srv,
        t4,
    )
    .await
    .unwrap();
    assert!(res4.pushed.contains(&"a2.txt".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        std::fs::read_to_string(srv.dir.path().join("a2.txt")).unwrap(),
        "from-client-2"
    );

    let (t5, _r5) = mpsc::channel(256);
    let res5 = session::run_sync_session(
        c1.crypto.clone(),
        c1.storage.clone(),
        c1.path(),
        addr,
        c1.folder_id,
        &id_srv,
        t5,
    )
    .await
    .unwrap();
    assert!(res5.pulled.contains(&"a2.txt".to_string()));
    assert_eq!(
        std::fs::read_to_string(c1.dir.path().join("a2.txt")).unwrap(),
        "from-client-2"
    );
}

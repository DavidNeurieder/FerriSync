use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::protocol::{frame_message, FileChunk, Index, IndexEntry, SyncMessage};
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use ferrisync_core::sync_engine::SyncEngine;
use ferrisync_core::DeviceInfo;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

const CHUNK_SIZE: usize = 64 * 1024;

/// Test: basic file sync between two peers
#[tokio::test]
async fn test_basic_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let test_file = dir_a.path().join("hello.txt");
    std::fs::write(&test_file, b"Hello from peer A").unwrap();

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

    assert!(dir_a.path().join("from_b.txt").exists(), "from_b.txt should exist on peer A");
    assert!(dir_b.path().join("from_a.txt").exists(), "from_a.txt should exist on peer B");
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
    std::fs::write(dir_a.path().join("subdir").join("nested.txt"), b"Nested file from A").unwrap();
    std::fs::write(dir_b.path().join("other.txt"), b"Other file from B").unwrap();

    let crypto_a = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_b = Arc::new(CryptoProvider::generate().unwrap());

    let storage_a = Arc::new(Storage::open(&dir_a.path().join("metadata.db")).unwrap());
    let storage_b = Arc::new(Storage::open(&dir_b.path().join("metadata.db")).unwrap());

    let (tx_a, _rx_a) = mpsc::channel(256);

    let dev_a_id = uuid::Uuid::new_v4();
    let dev_b_id = uuid::Uuid::new_v4();

    storage_a.upsert_device(&dev_b_id.to_string(), "peer_b", None).unwrap();
    storage_b.upsert_device(&dev_a_id.to_string(), "peer_a", None).unwrap();

    let folder_id_a = storage_a
        .add_sync_folder(dir_a.path().to_str().unwrap(), &dev_b_id.to_string(), "bidirectional")
        .unwrap();
    let folder_id_b = storage_b
        .add_sync_folder(dir_b.path().to_str().unwrap(), &dev_a_id.to_string(), "bidirectional")
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
                        let _ = session::handle_server_session(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            c, s, &p, folder_id_b, ev,
                        ).await;
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
    assert!(result.pushed.contains(&"root.txt".to_string()), "root.txt should be pushed");
    assert!(result.pushed.contains(&"subdir/nested.txt".to_string()), "subdir/nested.txt should be pushed");
    assert!(result.pulled.contains(&"other.txt".to_string()), "other.txt should be pulled");

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Verify A got B's files
    assert!(dir_a.path().join("other.txt").exists(), "other.txt should exist on A");
    assert_eq!(
        std::fs::read_to_string(dir_a.path().join("other.txt")).unwrap(),
        "Other file from B"
    );

    // Verify B got A's files
    assert!(dir_b.path().join("root.txt").exists(), "root.txt should exist on B");
    assert_eq!(
        std::fs::read_to_string(dir_b.path().join("root.txt")).unwrap(),
        "Root file from A"
    );
    assert!(dir_b.path().join("subdir").join("nested.txt").exists(), "nested.txt should exist on B");
    assert_eq!(
        std::fs::read_to_string(dir_b.path().join("subdir").join("nested.txt")).unwrap(),
        "Nested file from A"
    );
}

/// Send a file as framed chunks over a TLS stream.
async fn send_file_chunks_tls(
    conn: &mut tokio_rustls::TlsStream<tokio::net::TcpStream>,
    path: &str,
    data: &[u8],
    folder_id: &str,
) -> Result<(), anyhow::Error> {
    let total_size = data.len() as u64;
    let mut offset = 0u64;
    while offset < total_size {
        let end = (offset as usize + CHUNK_SIZE).min(total_size as usize);
        let chunk = FileChunk {
            folder_id: folder_id.to_string(),
            path: path.to_string(),
            offset,
            data: data[offset as usize..end].to_vec(),
            total_size,
        };
        let framed = frame_message(&SyncMessage::FileChunk(chunk))?;
        conn.write_all(&framed).await?;
        offset = end as u64;
    }
    Ok(())
}

/// A minimal server handler compatible with SyncEngine::sync_folder().
/// SyncEngine is pull-only: it sends Index, receives Index, sends FileRequest,
/// receives FileChunks. This server never pushes unsolicited chunks.
async fn handle_compatible_server(
    tcp: tokio::net::TcpStream,
    crypto: Arc<CryptoProvider>,
    local_path: String,
    folder_id: String,
) {
    let config = crypto.server_config().await.unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(config);
    let mut tls = match acceptor.accept(tcp).await {
        Ok(t) => tokio_rustls::TlsStream::Server(t),
        Err(e) => {
            eprintln!("TLS accept failed: {e}");
            return;
        }
    };

    let mut len_buf = [0u8; 4];

    // Read client Index
    if tls.read_exact(&mut len_buf).await.is_err() { return; }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    if tls.read_exact(&mut payload).await.is_err() { return; }
    let msg: SyncMessage = bincode::deserialize(&payload).unwrap();
    let _client_index = match msg {
        SyncMessage::Index(idx) => idx,
        _ => { return; }
    };

    // Build and send our Index by scanning local_path
    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&local_path) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() { continue; }
            if !path.is_file() { continue; }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if let Ok(data) = std::fs::read(&path) {
                let hash = blake3::hash(&data).as_bytes().to_vec();
                let meta = std::fs::metadata(&path).unwrap();
                let mtime = meta.modified().unwrap()
                    .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
                entries.push(IndexEntry {
                    path: name,
                    local_version: mtime as u64,
                    remote_version: 0,
                    mtime,
                    size: meta.len(),
                    hash,
                });
            }
        }
    }

    let our_index = Index { folder_id: folder_id.clone(), entries };
    let framed = frame_message(&SyncMessage::Index(our_index)).unwrap();
    tls.write_all(&framed).await.unwrap();

    // Read client's next message (FileRequest or Ack)
    if tls.read_exact(&mut len_buf).await.is_err() { return; }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    if tls.read_exact(&mut payload).await.is_err() { return; }
    let msg: SyncMessage = bincode::deserialize(&payload).unwrap();

    match msg {
        SyncMessage::FileRequest(req) => {
            for path in &req.paths {
                let file_path = PathBuf::from(&local_path).join(path);
                if let Ok(data) = tokio::fs::read(&file_path).await {
                    let _ = send_file_chunks_tls(&mut tls, path, &data, &folder_id).await;
                }
            }
            // Wait for Acks for each requested file
            for _ in 0..req.paths.len() {
                if tls.read_exact(&mut len_buf).await.is_err() { return; }
                let len = u32::from_be_bytes(len_buf) as usize;
                let mut payload = vec![0u8; len];
                if tls.read_exact(&mut payload).await.is_err() { return; }
                let _msg: SyncMessage = bincode::deserialize(&payload).unwrap();
            }
        }
        SyncMessage::Ack(_ack) => {
            // Client has nothing to pull
        }
        _ => {}
    }
}

/// Test: SyncEngine::sync_folder() as pull-only client with compatible server
#[tokio::test]
async fn test_cli_code_path_sync() {
    let dir_client = tempfile::tempdir().unwrap();
    let dir_server = tempfile::tempdir().unwrap();

    // Server has a file the client needs
    std::fs::write(dir_server.path().join("server_file.txt"), b"Hello from server").unwrap();

    // Client has no files

    let crypto_server = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_client = Arc::new(CryptoProvider::generate().unwrap());

    let storage_client = Arc::new(
        Storage::open(&dir_client.path().join("metadata.db")).unwrap(),
    );

    let dev_id = uuid::Uuid::new_v4().to_string();
    storage_client.upsert_device(&dev_id, "server", None).unwrap();

    let server_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cs = crypto_server.clone();
    let ps = dir_server.path().to_str().unwrap().to_string();
    let fid = "test-folder-cli".to_string();

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    handle_compatible_server(
                        tcp,
                        cs.clone(),
                        ps.clone(),
                        fid.clone(),
                    ).await;
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Set up SyncEngine as client
    let device_info = DeviceInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: "test-client".to_string(),
        cert_fingerprint: crypto_client.fingerprint().await,
    };
    let engine = SyncEngine::new(
        storage_client.clone(),
        crypto_client.clone(),
        device_info,
    );

    let local_folder_id = storage_client
        .add_sync_folder(
            dir_client.path().to_str().unwrap(),
            &dev_id,
            "pull",
        )
        .unwrap();

    let result = engine
        .sync_folder(
            local_folder_id,
            dir_client.path().to_str().unwrap(),
            actual_addr,
            &dev_id,
        )
        .await;

    assert!(result.is_ok(), "sync_folder should succeed: {:?}", result.err());

    // Give the server time to process
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify client pulled server_file.txt
    let client_file = dir_client.path().join("server_file.txt");
    assert!(client_file.exists(), "server_file.txt should exist on client");
    let content = std::fs::read_to_string(&client_file).unwrap();
    assert_eq!(content, "Hello from server");
}

/// Test: SyncEngine::sync_folder() pull when both sides have files
#[tokio::test]
async fn test_cli_code_path_conflict_resolution() {
    let dir_client = tempfile::tempdir().unwrap();
    let dir_server = tempfile::tempdir().unwrap();

    // Both have the same file but server's is newer (created after a delay)
    std::fs::write(dir_client.path().join("shared.txt"), b"Client version (older)").unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    std::fs::write(dir_server.path().join("shared.txt"), b"Server version").unwrap();

    let crypto_server = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_client = Arc::new(CryptoProvider::generate().unwrap());

    let storage_client = Arc::new(
        Storage::open(&dir_client.path().join("metadata.db")).unwrap(),
    );

    let dev_id = uuid::Uuid::new_v4().to_string();
    storage_client.upsert_device(&dev_id, "server", None).unwrap();

    let server_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cs = crypto_server.clone();
    let ps = dir_server.path().to_str().unwrap().to_string();
    let fid = "test-folder-conflict".to_string();

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    handle_compatible_server(tcp, cs.clone(), ps.clone(), fid.clone()).await;
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let device_info = DeviceInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: "test-client".to_string(),
        cert_fingerprint: crypto_client.fingerprint().await,
    };
    let engine = SyncEngine::new(
        storage_client.clone(),
        crypto_client.clone(),
        device_info,
    );

    let local_folder_id = storage_client
        .add_sync_folder(
            dir_client.path().to_str().unwrap(),
            &dev_id,
            "pull",
        )
        .unwrap();

    let result = engine
        .sync_folder(
            local_folder_id,
            dir_client.path().to_str().unwrap(),
            actual_addr,
            &dev_id,
        )
        .await;

    assert!(result.is_ok(), "sync_folder should succeed: {:?}", result.err());

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Client should have pulled server's version (newer mtime)
    let shared_file = dir_client.path().join("shared.txt");
    let content = std::fs::read_to_string(&shared_file).unwrap();
    assert_eq!(content, "Server version", "newer mtime should win");
}

/// Test: empty sync (no files on either side)
#[tokio::test]
async fn test_cli_code_path_empty_sync() {
    let dir_client = tempfile::tempdir().unwrap();
    let dir_server = tempfile::tempdir().unwrap();

    // Neither side has files

    let crypto_server = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_client = Arc::new(CryptoProvider::generate().unwrap());

    let storage_client = Arc::new(
        Storage::open(&dir_client.path().join("metadata.db")).unwrap(),
    );

    let dev_id = uuid::Uuid::new_v4().to_string();
    storage_client.upsert_device(&dev_id, "server", None).unwrap();

    let server_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cs = crypto_server.clone();
    let ps = dir_server.path().to_str().unwrap().to_string();
    let fid = "test-folder-empty".to_string();

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    handle_compatible_server(tcp, cs.clone(), ps.clone(), fid.clone()).await;
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let device_info = DeviceInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: "test-client".to_string(),
        cert_fingerprint: crypto_client.fingerprint().await,
    };
    let engine = SyncEngine::new(
        storage_client.clone(),
        crypto_client.clone(),
        device_info,
    );

    let local_folder_id = storage_client
        .add_sync_folder(
            dir_client.path().to_str().unwrap(),
            &dev_id,
            "pull",
        )
        .unwrap();

    let result = engine
        .sync_folder(
            local_folder_id,
            dir_client.path().to_str().unwrap(),
            actual_addr,
            &dev_id,
        )
        .await;

    assert!(result.is_ok(), "empty sync should succeed: {:?}", result.err());
}

/// Test: SyncEngine with small file transfer (single chunk)
#[tokio::test]
async fn test_cli_code_path_small_file() {
    let dir_client = tempfile::tempdir().unwrap();
    let dir_server = tempfile::tempdir().unwrap();

    let data = b"Small file content that fits in one chunk";
    std::fs::write(dir_server.path().join("small.txt"), data).unwrap();

    let crypto_server = Arc::new(CryptoProvider::generate().unwrap());
    let crypto_client = Arc::new(CryptoProvider::generate().unwrap());

    let storage_client = Arc::new(
        Storage::open(&dir_client.path().join("metadata.db")).unwrap(),
    );

    let dev_id = uuid::Uuid::new_v4().to_string();
    storage_client.upsert_device(&dev_id, "server", None).unwrap();

    let server_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let cs = crypto_server.clone();
    let ps = dir_server.path().to_str().unwrap().to_string();
    let fid = "test-folder-small".to_string();

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    handle_compatible_server(tcp, cs.clone(), ps.clone(), fid.clone()).await;
                }
                Err(e) => eprintln!("accept error: {e}"),
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let device_info = DeviceInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: "test-client".to_string(),
        cert_fingerprint: crypto_client.fingerprint().await,
    };
    let engine = SyncEngine::new(
        storage_client.clone(),
        crypto_client.clone(),
        device_info,
    );

    let local_folder_id = storage_client
        .add_sync_folder(
            dir_client.path().to_str().unwrap(),
            &dev_id,
            "pull",
        )
        .unwrap();

    let result = engine
        .sync_folder(
            local_folder_id,
            dir_client.path().to_str().unwrap(),
            actual_addr,
            &dev_id,
        )
        .await;

    assert!(result.is_ok(), "small file sync should succeed: {:?}", result.err());

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

    storage_a.upsert_device(&dev_b_id.to_string(), "peer_b", None).unwrap();
    storage_b.upsert_device(&dev_a_id.to_string(), "peer_a", None).unwrap();

    let folder_id_a = storage_a
        .add_sync_folder(dir_a.path().to_str().unwrap(), &dev_b_id.to_string(), "bidirectional")
        .unwrap();
    let folder_id_b = storage_b
        .add_sync_folder(dir_b.path().to_str().unwrap(), &dev_a_id.to_string(), "bidirectional")
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
                        let _ = session::handle_server_session(
                            &mut tokio_rustls::TlsStream::Server(tls),
                            c, s, &p, folder_id_b, ev,
                        ).await;
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

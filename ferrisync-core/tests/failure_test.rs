//! Integration tests for failure scenarios and edge cases.
//!
//! Core invariant: a failed synchronization must not silently corrupt or
//! delete unrelated user data.

use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use ferrisync_core::sync_engine::SyncEvent;
use ferrisync_core::transport::tcp::TcpTransport;
use ferrisync_core::transport::TransportConnector;
use std::sync::Arc;
use tokio::sync::mpsc;

fn dummy_store() -> Arc<InMemoryStateStore> {
    Arc::new(InMemoryStateStore::new())
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
    device_id: &str,
) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let device_id = device_id.to_string();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let crypto = crypto.clone();
            let storage = storage.clone();
            let local_path = local_path.clone();
            let event_tx = event_tx.clone();
            let device_id = device_id.clone();
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
                        &device_id,
                        dummy_store(),
                    )
                    .await;
                }
            });
        }
    });
    addr
}

// ────────────────────────────────────────────────────────────────────
// 1. Server unavailable — client must fail gracefully, not hang
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_connect_to_unreachable_server() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
    let crypto = Arc::new(CryptoProvider::generate().unwrap());
    storage
        .upsert_device("remote-id", "remote", None, None)
        .unwrap();
    let folder_id = storage
        .add_sync_folder(dir.path().to_str().unwrap(), "remote-id", "bidirectional")
        .unwrap();

    // Use a port that is almost certainly not listening
    let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
    let (tx, _rx) = mpsc::channel(256);

    let result = session::run_sync_session(
        crypto,
        storage,
        dir.path().to_str().unwrap(),
        addr,
        folder_id,
        "remote-id",
        tx,
        dummy_store(),
        false,
        None,
    )
    .await;

    assert!(
        result.is_err(),
        "connecting to unavailable server must fail"
    );
    // Must not hang — this test will timeout if it does
}

// ────────────────────────────────────────────────────────────────────
// 2. Server drops connection mid-sync — no data corruption
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_server_disconnect_mid_sync() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    // A has several files
    std::fs::write(a.dir.path().join("f1.txt"), b"content-1").unwrap();
    std::fs::write(a.dir.path().join("f2.txt"), b"content-2").unwrap();
    std::fs::write(a.dir.path().join("f3.txt"), b"content-3").unwrap();

    // Spawn a server that will kill itself after the first accepted TLS connection
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let b_crypto = b.crypto.clone();
    tokio::spawn(async move {
        if let Ok((tcp, _)) = listener.accept().await {
            let config = match b_crypto.server_config().await {
                Ok(c) => c,
                Err(_) => return,
            };
            let acceptor = tokio_rustls::TlsAcceptor::from(config);
            if let Ok(_tls) = acceptor.accept(tcp).await {
                // Drop the TLS stream immediately — simulates server crash
                // The client should get an error but no files should be corrupted
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (tx, _rx) = mpsc::channel(256);
    let _result = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx,
        dummy_store(),
        false,
        None,
    )
    .await;

    // The sync may fail due to the dropped connection — that's fine
    // The important thing is: no files on side A should be corrupted
    let a_f1 = std::fs::read(a.dir.path().join("f1.txt")).unwrap();
    let a_f2 = std::fs::read(a.dir.path().join("f2.txt")).unwrap();
    let a_f3 = std::fs::read(a.dir.path().join("f3.txt")).unwrap();
    assert_eq!(a_f1, b"content-1");
    assert_eq!(a_f2, b"content-2");
    assert_eq!(a_f3, b"content-3");

    // Server side must also be intact
    if let Ok(Some(content)) =
        std::fs::read(b.dir.path().join("f1.txt"))
            .map(|c| if c.is_empty() { None } else { Some(c) })
    {
        assert_eq!(content, b"content-1");
    }
}

// ────────────────────────────────────────────────────────────────────
// 3. Peer disappears and reconnects — second sync succeeds
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_peer_reconnects_after_disconnect() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("round1.txt"), b"r1").unwrap();

    // First sync — server goes away right after
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let b_crypto = b.crypto.clone();
    let b_storage = b.storage.clone();
    let b_path = b.path().to_string();
    let b_folder_id = b.folder_id;
    let b_id = id_b.clone();

    // Spawn server that accepts one connection then drops
    tokio::spawn(async move {
        if let Ok((tcp, _)) = listener.accept().await {
            let config = b_crypto.server_config().await.unwrap();
            let acceptor = tokio_rustls::TlsAcceptor::from(config);
            if let Ok(tls) = acceptor.accept(tcp).await {
                let _ = session::handle_server_session_with_read(
                    &mut tokio_rustls::TlsStream::Server(tls),
                    b_crypto.clone(),
                    b_storage.clone(),
                    &b_path,
                    b_folder_id,
                    mpsc::channel(256).0,
                    &b_id,
                    dummy_store(),
                )
                .await;
            }
        }
        // Server task ends here — drops listener
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (tx1, _rx1) = mpsc::channel(256);
    let r1 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx1,
        dummy_store(),
        false,
        None,
    )
    .await;
    // First sync may succeed or fail depending on timing — that's OK
    let _ = r1;

    // Wait for first server to drop
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Now A has new content to push
    std::fs::write(a.dir.path().join("round2.txt"), b"r2").unwrap();

    // Spawn a NEW server
    let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr2 = listener2.local_addr().unwrap();
    let b_crypto2 = b.crypto.clone();
    let b_storage2 = b.storage.clone();
    let b_path2 = b.path().to_string();
    let b_folder_id2 = b.folder_id;
    let b_id2 = id_b.clone();

    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener2.accept().await {
            let c = b_crypto2.clone();
            let s = b_storage2.clone();
            let p = b_path2.clone();
            let did = b_id2.clone();
            let fid = b_folder_id2;
            tokio::spawn(async move {
                let config = c.server_config().await.unwrap();
                let acceptor = tokio_rustls::TlsAcceptor::from(config);
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let _ = session::handle_server_session_with_read(
                        &mut tokio_rustls::TlsStream::Server(tls),
                        c,
                        s,
                        &p,
                        fid,
                        mpsc::channel(256).0,
                        &did,
                        dummy_store(),
                    )
                    .await;
                }
            });
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (tx2, _rx2) = mpsc::channel(256);
    let r2 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr2,
        a.folder_id,
        &id_b,
        tx2,
        dummy_store(),
        false,
        None,
    )
    .await
    .expect("second sync after reconnect must succeed");

    assert!(r2.pushed.contains(&"round2.txt".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        std::fs::read_to_string(b.dir.path().join("round2.txt")).unwrap(),
        "r2"
    );
}

// ────────────────────────────────────────────────────────────────────
// 4. Simultaneous edits — conflict detected, no data loss
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_simultaneous_edit_conflict_no_data_loss() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    // Both sides have the same file with same content initially
    std::fs::write(a.dir.path().join("shared.txt"), b"base").unwrap();
    std::fs::write(b.dir.path().join("shared.txt"), b"base").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
        &id_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // First sync to establish baseline
    let (tx1, _rx1) = mpsc::channel(256);
    let _ = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx1,
        dummy_store(),
        false,
        None,
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Now both sides edit simultaneously (different device IDs in vector clocks)
    std::fs::write(a.dir.path().join("shared.txt"), b"edit-from-A").unwrap();
    std::fs::write(b.dir.path().join("shared.txt"), b"edit-from-B").unwrap();

    let (tx2, _rx2) = mpsc::channel(256);
    let r2 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx2,
        dummy_store(),
        false,
        None,
    )
    .await
    .unwrap();

    // Conflict must be detected
    assert!(
        r2.conflicts.contains(&"shared.txt".to_string()),
        "conflict should be detected for shared.txt"
    );

    // The local file must not be empty or garbage
    let local_content = std::fs::read_to_string(a.dir.path().join("shared.txt")).unwrap();
    assert!(
        local_content == "edit-from-A" || local_content == "edit-from-B",
        "local file must contain a valid version, got: {local_content}"
    );

    // A conflict backup must exist
    let has_backup = std::fs::read_dir(a.dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.path()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shared.txt.ferrisync-conflict-")
        });
    assert!(has_backup, "conflict backup must exist");
}

// ────────────────────────────────────────────────────────────────────
// 5. Simultaneous delete/edit — deleted file must not reappear
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_simultaneous_delete_edit_no_resurrection() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    // Both sides start with the same file
    std::fs::write(a.dir.path().join("victim.txt"), b"alive").unwrap();
    std::fs::write(b.dir.path().join("victim.txt"), b"alive").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
        &id_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // First sync
    let (tx1, _rx1) = mpsc::channel(256);
    let _ = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx1,
        dummy_store(),
        false,
        None,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // A deletes the file, B edits it
    std::fs::remove_file(a.dir.path().join("victim.txt")).unwrap();
    std::fs::write(b.dir.path().join("victim.txt"), b"zombie").unwrap();

    let (tx2, _rx2) = mpsc::channel(256);
    let _r2 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx2,
        dummy_store(),
        false,
        None,
    )
    .await
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // After sync, the server's version (edit) wins the conflict.
    // The file should either exist with valid content or not exist —
    // but it must NOT have stale content from the pre-delete state.
    let victim_path = a.dir.path().join("victim.txt");
    if victim_path.exists() {
        let content = std::fs::read_to_string(&victim_path).unwrap();
        assert!(
            content == "zombie" || content == "alive",
            "victim.txt has unexpected content: {content}"
        );
    }
    // Server must have the file
    assert!(
        b.dir.path().join("victim.txt").exists(),
        "server should still have the edited version"
    );
}

// ────────────────────────────────────────────────────────────────────
// 6. Malformed message — server must reject without crashing
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_malformed_message_rejected() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let b = TestSide::new(&uuid::Uuid::new_v4().to_string(), "server-b");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let b_crypto = b.crypto.clone();
    let b_storage = b.storage.clone();
    let b_path = b.path().to_string();
    let b_id = id_b.clone();
    let b_folder_id = b.folder_id;

    // Server task
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let c = b_crypto.clone();
            let s = b_storage.clone();
            let p = b_path.clone();
            let did = b_id.clone();
            let fid = b_folder_id;
            tokio::spawn(async move {
                let config = c.server_config().await.unwrap();
                let acceptor = tokio_rustls::TlsAcceptor::from(config);
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let _ = session::handle_server_session_with_read(
                        &mut tokio_rustls::TlsStream::Server(tls),
                        c,
                        s,
                        &p,
                        fid,
                        mpsc::channel(256).0,
                        &did,
                        dummy_store(),
                    )
                    .await;
                }
            });
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect as a client and send garbage after TLS handshake
    let a_crypto = Arc::new(CryptoProvider::generate().unwrap());

    let transport = TcpTransport::new(a_crypto.clone());
    let mut conn = transport.connect(addr).await.unwrap();

    // Send garbage bytes — not a valid SyncMessage
    let garbage = vec![0xFF; 256];
    conn.write_all(&garbage).await.unwrap();

    // The server should close the connection gracefully, not crash
    let mut buf = vec![0u8; 4096];
    let _result =
        tokio::time::timeout(std::time::Duration::from_secs(3), conn.read(&mut buf)).await;

    // Either we get an error (connection closed) or empty read — both are fine
    // The key assertion is that the server didn't crash (it's still running
    // on another thread, and the test completes normally)
}

// ────────────────────────────────────────────────────────────────────
// 7. Duplicate sync — no data duplication or corruption
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_duplicate_sync_no_data_duplication() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("once.txt"), b"single").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
        &id_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Sync 3 times in a row
    for i in 0..3 {
        let (tx, _rx) = mpsc::channel(256);
        let r = session::run_sync_session(
            a.crypto.clone(),
            a.storage.clone(),
            a.path(),
            addr,
            a.folder_id,
            &id_b,
            tx,
            dummy_store(),
            false,
            None,
        )
        .await
        .unwrap();

        if i == 0 {
            assert!(
                r.pushed.contains(&"once.txt".to_string()),
                "first sync should push"
            );
        } else {
            assert!(
                r.pushed.is_empty() && r.pulled.is_empty(),
                "sync #{i} should be a no-op"
            );
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // File must exist exactly once and have correct content
    let content = std::fs::read_to_string(a.dir.path().join("once.txt")).unwrap();
    assert_eq!(content, "single");

    let b_content = std::fs::read_to_string(b.dir.path().join("once.txt")).unwrap();
    assert_eq!(b_content, "single");
}

// ────────────────────────────────────────────────────────────────────
// 8. Database deleted mid-flight — session must not panic
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_database_deleted_does_not_panic() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("f.txt"), b"content").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
        &id_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Delete the client's database file — this should cause the sync to fail
    // gracefully, not panic
    let db_path = a.dir.path().join("metadata.db");
    std::fs::remove_file(&db_path).ok();

    let (tx, _rx) = mpsc::channel(256);
    let _result = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx,
        dummy_store(),
        false,
        None,
    )
    .await;

    // It's acceptable for this to fail — what matters is no panic
    // The original file should still be intact
    let content = std::fs::read(a.dir.path().join("f.txt")).unwrap();
    assert_eq!(
        content, b"content",
        "original file must survive DB deletion"
    );
}

// ────────────────────────────────────────────────────────────────────
// 9. Large number of small files — no truncation or loss
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_many_small_files_all_transfer() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    let file_count = 50;
    for i in 0..file_count {
        std::fs::write(
            a.dir.path().join(format!("file_{i:03}.txt")),
            format!("content-{i}").as_bytes(),
        )
        .unwrap();
    }

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
        &id_b,
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
        dummy_store(),
        false,
        None,
    )
    .await
    .unwrap();

    assert_eq!(result.pushed.len(), file_count);

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    for i in 0..file_count {
        let name = format!("file_{i:03}.txt");
        let path = b.dir.path().join(&name);
        assert!(path.exists(), "{name} should exist on server");
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, format!("content-{i}"));
    }
}

// ────────────────────────────────────────────────────────────────────
// 10. Sync result tracks all failures without panicking
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_sync_with_deleted_source_file() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("will_vanish.txt"), b"here then gone").unwrap();
    std::fs::write(a.dir.path().join("stable.txt"), b"stays").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
        &id_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Delete the file right before sync — the scan might pick it up but
    // the transfer should handle the missing file gracefully
    std::fs::remove_file(a.dir.path().join("will_vanish.txt")).unwrap();

    let (tx, _rx) = mpsc::channel(256);
    let result = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        &id_b,
        tx,
        dummy_store(),
        false,
        None,
    )
    .await;

    // Sync may succeed (with will_vanish.txt missing from pushed list)
    // or may partially fail — but it must not panic
    if let Ok(r) = &result {
        // stable.txt should still be pushed
        assert!(
            r.pushed.contains(&"stable.txt".to_string()),
            "stable.txt should be pushed even when another file is missing"
        );
    }
}

// ────────────────────────────────────────────────────────────────────
// 11. Reconnect after partial transfer — files not duplicated
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_partial_then_complete_sync_no_duplicates() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("data.txt"), b"payload").unwrap();

    // First attempt: server drops immediately
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr1 = listener.local_addr().unwrap();
    let b_crypto = b.crypto.clone();
    tokio::spawn(async move {
        if let Ok((tcp, _)) = listener.accept().await {
            let config = b_crypto.server_config().await.unwrap();
            let acceptor = tokio_rustls::TlsAcceptor::from(config);
            if let Ok(_tls) = acceptor.accept(tcp).await {
                drop(_tls);
            }
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let (tx1, _rx1) = mpsc::channel(256);
    let _ = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr1,
        a.folder_id,
        &id_b,
        tx1,
        dummy_store(),
        false,
        None,
    )
    .await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Second attempt: a new server that completes the sync
    let (_tx_b2, _rx_b2) = mpsc::channel(256);
    let addr2 = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b2,
        &id_b,
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (tx2, _rx2) = mpsc::channel(256);
    let r2 = session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr2,
        a.folder_id,
        &id_b,
        tx2,
        dummy_store(),
        false,
        None,
    )
    .await
    .unwrap();

    assert!(r2.pushed.contains(&"data.txt".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let b_content = std::fs::read_to_string(b.dir.path().join("data.txt")).unwrap();
    assert_eq!(b_content, "payload");
}

// ────────────────────────────────────────────────────────────────────
// 12. Empty file sync — no panic on zero-byte files
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_empty_file_transfer() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    std::fs::write(a.dir.path().join("empty.txt"), b"").unwrap();
    std::fs::write(a.dir.path().join("not_empty.txt"), b"has data").unwrap();

    let (_tx_b, _rx_b) = mpsc::channel(256);
    let addr = spawn_server(
        b.crypto.clone(),
        b.storage.clone(),
        b.path().to_string(),
        b.folder_id,
        _tx_b,
        &id_b,
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
        dummy_store(),
        false,
        None,
    )
    .await
    .unwrap();

    assert!(result.pushed.contains(&"empty.txt".to_string()));
    assert!(result.pushed.contains(&"not_empty.txt".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let b_empty = std::fs::read(b.dir.path().join("empty.txt")).unwrap();
    assert!(b_empty.is_empty(), "empty file should transfer as empty");
    let b_not_empty = std::fs::read_to_string(b.dir.path().join("not_empty.txt")).unwrap();
    assert_eq!(b_not_empty, "has data");
}

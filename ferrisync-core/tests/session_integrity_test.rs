//! Deterministic Milestone 1 session-integrity tests.
//!
//! These drive `run_sync_session` against a hand-scripted peer so the exact
//! message sequence is under test control (no external network, no reliance on
//! timing). They assert the Step 1.1/1.2 invariant:
//!
//! * an interrupted or timed-out transfer is reported as a failure, never as
//!   success;
//! * a missing acknowledgement cannot yield a complete result;
//! * a malformed remote index or a transfer that fails validation is rejected
//!   before it touches the filesystem;
//! * a genuinely empty, fully confirmed sync still succeeds.

use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::protocol::{
    parse_frame, FileChunk, Index, IndexEntry, SyncMessage, MAX_CONTROL_FRAME,
};
use ferrisync_core::protocol_v2::hello::Hello;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use ferrisync_core::DeviceInfo;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;

/// A server-side TLS stream, as the scripted peers speak on.
type PeerStream = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;

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

/// Read one length-prefixed frame from a stream.
async fn read_msg<S>(stream: &mut S) -> anyhow::Result<SyncMessage>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_CONTROL_FRAME {
        anyhow::bail!("oversized control frame: {len}");
    }
    let mut frame = vec![0u8; 4 + len];
    frame[..4].copy_from_slice(&header);
    stream.read_exact(&mut frame[4..]).await?;
    let (msg, _) = parse_frame(&frame)?;
    Ok(msg)
}

/// Write one length-prefixed frame to a stream.
async fn write_msg<S>(stream: &mut S, msg: &SyncMessage) -> anyhow::Result<()>
where
    S: tokio::io::AsyncWrite + Unpin,
{
    stream
        .write_all(&ferrisync_core::protocol::frame_message(msg)?)
        .await?;
    Ok(())
}

fn index_entry(path: &str, size: u64, hash: &[u8]) -> IndexEntry {
    IndexEntry {
        path: path.to_string(),
        // A positive version makes a remote-only file reconcile as a download.
        local_version: 1,
        remote_version: 0,
        mtime: 1,
        size,
        hash: hash.to_vec(),
    }
}

fn empty_index(device_id: &str) -> Index {
    Index {
        folder_id: "1".into(),
        device_id: device_id.to_string(),
        remote_path: None,
        entries: Vec::new(),
    }
}

/// Bind a scripted peer that performs the Hello/Index handshake and then runs
/// `script` with the live stream. Returns the address to connect to.
async fn spawn_scripted_peer<F, Fut>(
    crypto: Arc<CryptoProvider>,
    index: Index,
    script: F,
) -> std::net::SocketAddr
where
    F: FnOnce(PeerStream) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (tcp, _) = match listener.accept().await {
            Ok(v) => v,
            Err(_) => return,
        };
        let config = match crypto.server_config().await {
            Ok(c) => c,
            Err(_) => return,
        };
        let acceptor = TlsAcceptor::from(config);
        let mut stream = match acceptor.accept(tcp).await {
            Ok(s) => s,
            Err(_) => return,
        };
        // Client Hello, then answer with our own.
        if read_msg(&mut stream).await.is_err() {
            return;
        }
        let hello = Hello::from_device(
            &DeviceInfo {
                id: ferrisync_core::crypto::cert_to_device_id(&crypto.certificate().await),
                name: "scripted".into(),
                cert_fingerprint: Vec::new(),
            },
            &crypto,
            Vec::new(),
        )
        .await;
        if write_msg(&mut stream, &SyncMessage::Hello(hello))
            .await
            .is_err()
        {
            return;
        }
        if write_msg(&mut stream, &SyncMessage::Index(index))
            .await
            .is_err()
        {
            return;
        }
        script(stream).await;
    });
    addr
}

/// Bind a real server (the production handler) for one connection.
async fn spawn_real_server(b: &TestSide, peer_id: &str) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let crypto = b.crypto.clone();
    let storage = b.storage.clone();
    let local_path = b.path().to_string();
    let folder_id = b.folder_id;
    let device_id = peer_id.to_string();
    tokio::spawn(async move {
        let (tcp, _) = match listener.accept().await {
            Ok(v) => v,
            Err(_) => return,
        };
        let config = match crypto.server_config().await {
            Ok(c) => c,
            Err(_) => return,
        };
        let acceptor = TlsAcceptor::from(config);
        if let Ok(tls) = acceptor.accept(tcp).await {
            let _ = session::handle_server_session_with_read(
                &mut tokio_rustls::TlsStream::Server(tls),
                crypto,
                storage,
                &local_path,
                folder_id,
                mpsc::channel(256).0,
                &device_id,
                dummy_store(),
            )
            .await;
        }
    });
    addr
}

async fn run_client(
    a: &TestSide,
    addr: std::net::SocketAddr,
    peer_id: &str,
) -> anyhow::Result<ferrisync_core::SyncResult> {
    let (tx, _rx) = mpsc::channel(256);
    session::run_sync_session(
        a.crypto.clone(),
        a.storage.clone(),
        a.path(),
        addr,
        a.folder_id,
        peer_id,
        tx,
        dummy_store(),
        false,
        None,
    )
    .await
}

// ────────────────────────────────────────────────────────────────────
// An interrupted transfer must be a failure, never a success.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn interrupted_transfer_is_not_reported_as_success() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    std::fs::write(a.dir.path().join("f1.txt"), b"payload").unwrap();

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    // Handshake and index exchange complete, then the peer vanishes with the
    // client's upload still unacknowledged.
    let addr = spawn_scripted_peer(b_crypto, empty_index(&id_b), |_stream| async {}).await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(
        result.is_err(),
        "a peer that vanishes with an unacknowledged upload must not yield success"
    );
    assert_eq!(
        std::fs::read(a.dir.path().join("f1.txt")).unwrap(),
        b"payload",
        "the local source file must be untouched"
    );
}

// ────────────────────────────────────────────────────────────────────
// A missing acknowledgement, with the socket held open, must time out
// into a failure rather than silently succeeding.
// ────────────────────────────────────────────────────────────────────

#[tokio::test(start_paused = true)]
async fn missing_acknowledgement_times_out_into_failure() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    std::fs::write(a.dir.path().join("f1.txt"), b"payload").unwrap();

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let addr = spawn_scripted_peer(b_crypto, empty_index(&id_b), |stream| async move {
        // Hold the socket open without ever acknowledging the upload; the
        // paused clock advances to the session's idle timeout.
        let _held = stream;
        tokio::time::sleep(Duration::from_secs(3600)).await;
    })
    .await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(
        result.is_err(),
        "a session that never receives its acknowledgement must fail"
    );

    let sessions = a.storage.list_recent_sessions(1).unwrap();
    assert_eq!(sessions.len(), 1, "the failure must still be recorded");
    assert_eq!(
        sessions[0].status, "incomplete",
        "an unfinished session must be recorded as incomplete"
    );
}

// ────────────────────────────────────────────────────────────────────
// A malformed remote index is rejected before it drives any transfer.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn malformed_remote_index_is_rejected() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let mut index = empty_index(&id_b);
    index
        .entries
        .push(index_entry("../escape.txt", 4, &[0u8; 32]));

    let addr = spawn_scripted_peer(b_crypto, index, |_stream| async {}).await;
    let result = run_client(&a, addr, &id_b).await;

    assert!(
        result.is_err(),
        "a remote index carrying a traversal path must be rejected"
    );
    let escaped = a.dir.path().parent().unwrap().join("escape.txt");
    assert!(
        !escaped.exists(),
        "no path outside the sync root may ever be written"
    );
}

// ────────────────────────────────────────────────────────────────────
// A file whose bytes fail verification is rejected and never written.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn hash_mismatch_is_rejected_and_file_is_not_written() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    // Advertise a size/name we will actually send, but commit to a hash the
    // payload will never match.
    let mut index = empty_index(&id_b);
    index.entries.push(index_entry("x.txt", 5, &[0u8; 32]));

    let addr = spawn_scripted_peer(b_crypto, index, |mut stream| async move {
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path: "x.txt".into(),
                offset: 0,
                data: b"hello".to_vec(),
                total_size: 5,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(
        result.is_err(),
        "a file whose bytes do not match the agreed hash must fail the session"
    );
    assert!(
        !a.dir.path().join("x.txt").exists(),
        "a file that fails verification must never be written to disk"
    );

    let sessions = a.storage.list_recent_sessions(1).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].status, "incomplete",
        "a failed session must be recorded as incomplete"
    );
}

// ────────────────────────────────────────────────────────────────────
// A failed download must leave the destination's previous version intact
// (Milestone 2) and never strand a temporary file.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn failed_download_leaves_existing_destination_intact() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    // The client already holds an older copy of the file.
    std::fs::write(a.dir.path().join("x.txt"), b"OLD-LOCAL-VERSION").unwrap();

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    // Claim a remote version so new that the remote wins, so the client plans a
    // download for a path whose destination already exists.
    let mut index = empty_index(&id_b);
    index.entries.push(IndexEntry {
        path: "x.txt".into(),
        local_version: u64::MAX,
        remote_version: 0,
        mtime: 1,
        size: 5,
        hash: vec![0u8; 32], // the bytes we send can never match this
    });

    let addr = spawn_scripted_peer(b_crypto, index, |mut stream| async move {
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path: "x.txt".into(),
                offset: 0,
                data: b"hello".to_vec(),
                total_size: 5,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let result = run_client(&a, addr, &id_b).await;
    let err = result.expect_err("a download that fails verification must fail the session");
    // Prove the failure is the download's failed hash check (not, say, an
    // incidental timeout on the upload path).
    let session_err = err
        .downcast_ref::<ferrisync_core::sync_engine::session_transfer::SessionError>()
        .expect("the session must surface a typed SessionError");
    assert!(
        matches!(
            session_err,
            ferrisync_core::sync_engine::session_transfer::SessionError::HashMismatch { .. }
        ),
        "expected a hash mismatch, got: {session_err}"
    );
    assert_eq!(
        std::fs::read(a.dir.path().join("x.txt")).unwrap(),
        b"OLD-LOCAL-VERSION",
        "a failed download must leave the previous destination intact"
    );
    let leftovers: Vec<_> = std::fs::read_dir(a.dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(ferrisync_core::safe_fs::TEMP_PREFIX)
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "a failed download must not strand a temporary file: {leftovers:?}"
    );
}

#[tokio::test]
async fn unexpected_file_is_rejected() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    // We expect `expected.txt`, so the client stays in its receive loop…
    let mut index = empty_index(&id_b);
    index
        .entries
        .push(index_entry("expected.txt", 5, &[0u8; 32]));

    let addr = spawn_scripted_peer(b_crypto, index, |mut stream| async move {
        // …but the peer sends a path that was never agreed.
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path: "other.txt".into(),
                offset: 0,
                data: b"hello".to_vec(),
                total_size: 5,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(
        result.is_err(),
        "a file outside the agreed transfer set must fail the session"
    );
    assert!(!a.dir.path().join("other.txt").exists());
    assert!(!a.dir.path().join("expected.txt").exists());
}

// ────────────────────────────────────────────────────────────────────
// A genuinely empty sync is still allowed and recorded as complete.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn empty_sync_succeeds_and_records_complete() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");

    let addr = spawn_real_server(&b, &id_b).await;

    let result = run_client(&a, addr, &id_b).await;
    let r = result.expect("a fully confirmed empty sync must succeed");
    assert!(r.pushed.is_empty() && r.pulled.is_empty());

    let sessions = a.storage.list_recent_sessions(1).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].status, "complete",
        "a confirmed sync must be recorded as complete"
    );
}

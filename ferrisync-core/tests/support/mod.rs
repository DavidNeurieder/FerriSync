//! Shared harness for integration tests that drive `run_sync_session` against a
//! peer whose exact message sequence is under test control.
//!
//! This is not a test target by itself: files include it with `mod support;`.
//! Most helpers are only used by some of the integration binaries, so the whole
//! module allows dead code rather than warning per binary.
#![allow(dead_code)]

use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::protocol::{Index, IndexEntry, SyncMessage, MAX_CONTROL_FRAME};
use ferrisync_core::protocol_v2::hello::Hello;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use ferrisync_core::DeviceInfo;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// The TLS stream type the scripted/real peers talk over.
pub type PeerStream = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;

pub fn dummy_store() -> Arc<InMemoryStateStore> {
    Arc::new(InMemoryStateStore::new())
}

/// One side of a sync: its crypto identity, storage, and synchronization root.
pub struct TestSide {
    pub crypto: Arc<CryptoProvider>,
    pub storage: Arc<Storage>,
    pub dir: tempfile::TempDir,
    pub folder_id: i64,
}

impl TestSide {
    pub fn new(other_device_id: &str, other_name: &str) -> Self {
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

    pub fn path(&self) -> &str {
        self.dir.path().to_str().unwrap()
    }

    /// Write a file directly into this side's synchronization root.
    pub fn write(&self, name: &str, data: &[u8]) {
        std::fs::write(self.dir.path().join(name), data).unwrap();
    }

    /// Read a file directly from this side's synchronization root.
    pub fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.dir.path().join(name)).unwrap()
    }
}

/// Read one length-prefixed frame from a stream.
pub async fn read_msg<S>(stream: &mut S) -> anyhow::Result<SyncMessage>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_CONTROL_FRAME {
        anyhow::bail!("oversized control frame: {len}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    let mut framed = header.to_vec();
    framed.extend_from_slice(&body);
    let (msg, _) = ferrisync_core::protocol::parse_frame(&framed)?;
    Ok(msg)
}

/// Write one length-prefixed frame to a stream.
pub async fn write_msg<S>(stream: &mut S, msg: &SyncMessage) -> anyhow::Result<()>
where
    S: tokio::io::AsyncWrite + Unpin,
{
    let framed = ferrisync_core::protocol::frame_message(msg)?;
    stream.write_all(&framed).await?;
    Ok(())
}

/// A remote index entry the local side will be asked to receive.
pub fn index_entry(path: &str, size: u64, hash: &[u8]) -> ferrisync_core::protocol::IndexEntry {
    ferrisync_core::protocol::IndexEntry {
        path: path.to_string(),
        local_version: 1,
        remote_version: 0,
        mtime: 1,
        size,
        hash: hash.to_vec(),
    }
}

/// An index entry whose remote version dominates any local version, so the
/// local side plans a download even when it already holds the path.
pub fn index_entry_remote_wins(path: &str, size: u64, hash: &[u8]) -> IndexEntry {
    IndexEntry {
        path: path.to_string(),
        local_version: u64::MAX,
        remote_version: 0,
        mtime: 1,
        size,
        hash: hash.to_vec(),
    }
}

pub fn empty_index(device_id: &str) -> Index {
    Index {
        folder_id: "1".into(),
        device_id: device_id.to_string(),
        remote_path: None,
        entries: Vec::new(),
    }
}

/// Bind a scripted peer that performs the Hello/Index handshake and then runs
/// `script` with the live stream. Returns the address to connect to.
pub async fn spawn_scripted_peer<F, Fut>(
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
        let acceptor = tokio_rustls::TlsAcceptor::from(config);
        let mut stream = match acceptor.accept(tcp).await {
            Ok(s) => s,
            Err(_) => return,
        };
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

/// Like [`spawn_scripted_peer`], but runs `hook` after the client's index has
/// been read and before the peer's own index is sent. The hook can, for
/// example, mutate a file on the client between indexing and upload so the
/// stale-snapshot path is exercised deterministically.
pub async fn spawn_scripted_peer_with_hook<H, HFut, F, Fut>(
    crypto: Arc<CryptoProvider>,
    index: Index,
    hook: H,
    script: F,
) -> std::net::SocketAddr
where
    H: FnOnce() -> HFut + Send + 'static,
    HFut: std::future::Future<Output = ()> + Send + 'static,
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
        let acceptor = tokio_rustls::TlsAcceptor::from(config);
        let mut stream = match acceptor.accept(tcp).await {
            Ok(s) => s,
            Err(_) => return,
        };
        // Handshake order: read the client Hello, answer with ours, read the
        // client index, run the hook, then send our index.
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
        if read_msg(&mut stream).await.is_err() {
            return;
        }
        hook().await;
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

/// Bind a peer that completes the TLS handshake but does NOT speak the sync
/// protocol — the script owns every byte from then on. Used to test aborting
/// (or corrupting) the session at an arbitrary point.
pub async fn spawn_raw_peer<F, Fut>(crypto: Arc<CryptoProvider>, script: F) -> std::net::SocketAddr
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
        let acceptor = tokio_rustls::TlsAcceptor::from(config);
        if let Ok(stream) = acceptor.accept(tcp).await {
            script(stream).await;
        }
    });
    addr
}

/// Bind a real server (the production handler) for one connection.
pub async fn spawn_real_server(b: &TestSide, peer_id: &str) -> std::net::SocketAddr {
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
        let acceptor = tokio_rustls::TlsAcceptor::from(config);
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

/// A server that accepts many concurrent connections, each handled with the
/// production server-side session handler. Used to exercise simultaneous
/// sessions targeting the same destination.
pub async fn spawn_multi_server(
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    local_path: String,
    folder_id: i64,
    device_id: String,
) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let crypto = crypto.clone();
            let storage = storage.clone();
            let local_path = local_path.clone();
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
                        mpsc::channel(256).0,
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

pub async fn run_client(
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

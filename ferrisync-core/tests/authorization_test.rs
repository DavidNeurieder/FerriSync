//! Milestone 5 — device authorization regression tests.
//!
//! These drive the production serving path (`serve_folder` → `listen_for_sync`),
//! which is the single authorization boundary for file data. Every message that
//! serves or receives file bytes, or that enumerates shared folders, must come
//! from a peer whose TLS certificate maps to a device we have paired with.
//! mDNS never grants trust, and revoking a device (removing its row)
//! immediately revokes both syncing and shared-folder enumeration.

use ferrisync_core::crypto::{cert_to_device_id, CryptoProvider};
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::protocol::{frame_message, parse_frame, SyncMessage};
use ferrisync_core::protocol_v2::shared::SharedFolderInfo;
use ferrisync_core::protocol_v2::Hello;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::server::{serve_folder, PairPolicy};
use ferrisync_core::sync_engine::session;
use ferrisync_core::sync_engine::shared_folder::SharedFolderClient;
use ferrisync_core::transport::tcp::TcpTransport;
use ferrisync_core::transport::{TransportConnection, TransportConnector};
use ferrisync_core::{DeviceInfo, SyncResult};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Bound every peer round-trip so a server that silently drops an unauthorized
/// session can never hang the test.
const OP_TIMEOUT: Duration = Duration::from_secs(10);

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Derive a device's stable id and advertised info from its certificate.
async fn identity(crypto: &CryptoProvider, name: &str) -> DeviceInfo {
    let cert = crypto.certificate().await;
    DeviceInfo {
        id: cert_to_device_id(cert.as_ref()).to_string(),
        name: name.to_string(),
        cert_fingerprint: crypto.fingerprint().await,
    }
}

fn add_folder(storage: &Storage, dir: &Path, owner_id: &str) -> i64 {
    storage
        .add_sync_folder(dir.to_str().unwrap(), owner_id, "bidirectional")
        .unwrap()
}

async fn try_sync(
    crypto: &Arc<CryptoProvider>,
    storage: &Arc<Storage>,
    dir: &Path,
    folder_id: i64,
    peer_id: &str,
    addr: SocketAddr,
) -> anyhow::Result<SyncResult> {
    let (tx, _rx) = mpsc::channel(64);
    tokio::time::timeout(
        OP_TIMEOUT,
        session::run_sync_session(
            crypto.clone(),
            storage.clone(),
            dir.to_str().unwrap(),
            addr,
            folder_id,
            peer_id,
            tx,
            Arc::new(InMemoryStateStore::new()),
            false,
            None,
        ),
    )
    .await
    .expect("sync against an unauthorized peer must not hang")
}

async fn browse(
    crypto: &Arc<CryptoProvider>,
    addr: SocketAddr,
) -> anyhow::Result<Vec<SharedFolderInfo>> {
    tokio::time::timeout(
        OP_TIMEOUT,
        SharedFolderClient::new(crypto.clone(), addr).list_shared_folders(),
    )
    .await
    .expect("browse against an unauthorized peer must not hang")
}

/// Read one length-prefixed control frame, returning `None` when the peer
/// closed the connection first.
async fn read_control_frame(
    conn: &mut dyn TransportConnection,
) -> anyhow::Result<Option<SyncMessage>> {
    let mut len_buf = [0u8; 4];
    let mut filled = 0;
    while filled < 4 {
        let n = conn.read(&mut len_buf[filled..]).await?;
        if n == 0 {
            return Ok(None);
        }
        filled += n;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    let mut got = 0;
    while got < len {
        let n = conn.read(&mut payload[got..]).await?;
        if n == 0 {
            anyhow::bail!("connection closed mid-frame");
        }
        got += n;
    }
    let mut frame = Vec::with_capacity(4 + len);
    frame.extend_from_slice(&len_buf);
    frame.extend_from_slice(&payload);
    let (msg, _) = parse_frame(&frame)?;
    Ok(Some(msg))
}

#[tokio::test]
async fn unpaired_peer_is_denied_then_authorized_by_pairing_then_revoked() {
    let owner_dir = tempfile::tempdir().unwrap();
    let owner_data = tempfile::tempdir().unwrap();
    let requester_dir = tempfile::tempdir().unwrap();
    let requester_data = tempfile::tempdir().unwrap();
    let port = free_port();

    // ── Owner: identity plus a discoverable share holding one file. ──
    let owner_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let owner = identity(&owner_crypto, "owner").await;
    let owner_storage = Arc::new(Storage::open(&owner_data.path().join("metadata.db")).unwrap());
    owner_storage
        .upsert_device(&owner.id, "owner", None, None)
        .unwrap();
    std::fs::write(owner_dir.path().join("secret.txt"), b"owner only").unwrap();
    let owner_folder_id = add_folder(&owner_storage, owner_dir.path(), &owner.id);
    let guid = owner_storage
        .folder_guid(owner_folder_id)
        .unwrap()
        .expect("folder has a guid");
    owner_storage
        .share_folder(
            &guid,
            &owner.id,
            "shared",
            owner_dir.path().to_str().unwrap(),
        )
        .unwrap();

    let (server, _events) = serve_folder(
        owner_storage.clone(),
        owner_crypto.clone(),
        owner.clone(),
        owner_dir.path().to_str().unwrap().to_string(),
        port,
        PairPolicy::AutoAccept,
        Arc::new(InMemoryStateStore::new()),
    )
    .await
    .unwrap();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    // ── Requester: a distinct, initially unpaired identity with its own folder. ──
    let req_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let requester = identity(&req_crypto, "requester").await;
    let req_storage = Arc::new(Storage::open(&requester_data.path().join("metadata.db")).unwrap());
    req_storage
        .upsert_device(&requester.id, "requester", None, None)
        .unwrap();
    req_storage
        .upsert_device(&owner.id, "owner", None, None)
        .unwrap();
    let req_folder_id = add_folder(&req_storage, requester_dir.path(), &owner.id);

    // 1. Unpaired: the sync session is refused and no bytes leak.
    let denied = try_sync(
        &req_crypto,
        &req_storage,
        requester_dir.path(),
        req_folder_id,
        &requester.id,
        addr,
    )
    .await;
    assert!(
        denied.is_err(),
        "sync from an unpaired peer must be rejected, got {denied:?}"
    );
    assert!(
        !requester_dir.path().join("secret.txt").exists(),
        "an unpaired peer must not receive any file"
    );

    // 1b. Unpaired: shared-folder enumeration reveals nothing.
    let listed = browse(&req_crypto, addr).await.unwrap();
    assert!(
        listed.is_empty(),
        "an unpaired peer must not enumerate shared folders, got {listed:?}"
    );

    // 2. Pair the device. The owner auto-accepts and stores the requester's
    //    certificate, which is what authorizes it from here on.
    let pairing = PairingManager::new(req_crypto.clone(), req_storage.clone(), requester.clone());
    let paired_owner = tokio::time::timeout(OP_TIMEOUT, pairing.pair_with(addr))
        .await
        .expect("pairing must not hang")
        .unwrap();
    assert_eq!(paired_owner.id, owner.id);
    assert_eq!(paired_owner.name, "owner");

    // 2a. Authorized: enumeration now exposes the share and its real path.
    let listed = browse(&req_crypto, addr).await.unwrap();
    assert_eq!(
        listed.len(),
        1,
        "a paired peer should see exactly one share, got {listed:?}"
    );
    assert_eq!(listed[0].name, "shared");
    assert_eq!(
        listed[0].local_path,
        owner_dir.path().to_str().unwrap(),
        "a paired peer may see the owner's real remote path"
    );

    // 2b. Authorized: a sync pulls the owner's file.
    try_sync(
        &req_crypto,
        &req_storage,
        requester_dir.path(),
        req_folder_id,
        &requester.id,
        addr,
    )
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(requester_dir.path().join("secret.txt")).unwrap(),
        b"owner only"
    );

    // 3. Revoke: removing the device row revokes access immediately, even
    //    though the peer still holds a valid TLS certificate.
    owner_storage.remove_device(&requester.id).unwrap();
    std::fs::write(owner_dir.path().join("after-revoke.txt"), b"nope").unwrap();

    let denied = try_sync(
        &req_crypto,
        &req_storage,
        requester_dir.path(),
        req_folder_id,
        &requester.id,
        addr,
    )
    .await;
    assert!(
        denied.is_err(),
        "sync from a revoked peer must be rejected, got {denied:?}"
    );
    assert!(
        !requester_dir.path().join("after-revoke.txt").exists(),
        "a revoked peer must not receive new files"
    );

    let listed = browse(&req_crypto, addr).await.unwrap();
    assert!(
        listed.is_empty(),
        "a revoked peer must not enumerate shared folders, got {listed:?}"
    );

    server.stop().await;
}

#[tokio::test]
async fn hello_whose_id_disagrees_with_its_certificate_is_rejected() {
    // A peer must not be able to claim an identity other than the one bound to
    // its TLS certificate. This is the "mismatched identity" case: an attacker
    // presents its own (unpaired) certificate but a Hello claiming to be a
    // different, known device.
    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let port = free_port();

    let owner_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let owner = identity(&owner_crypto, "owner").await;
    let storage = Arc::new(Storage::open(&data.path().join("metadata.db")).unwrap());
    storage
        .upsert_device(&owner.id, "owner", None, None)
        .unwrap();
    add_folder(&storage, dir.path(), &owner.id);

    let (server, _events) = serve_folder(
        storage,
        owner_crypto,
        owner.clone(),
        dir.path().to_str().unwrap().to_string(),
        port,
        PairPolicy::AutoAccept,
        Arc::new(InMemoryStateStore::new()),
    )
    .await
    .unwrap();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();

    // The attacker's Hello claims the owner's id but carries the attacker cert.
    let attacker_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let spoofed = DeviceInfo {
        id: owner.id.clone(),
        name: "owner".to_string(),
        cert_fingerprint: attacker_crypto.fingerprint().await,
    };
    let hello = Hello::from_device(&spoofed, &attacker_crypto, vec![]).await;

    let transport = TcpTransport::new(attacker_crypto.clone());
    let mut conn = transport.connect(addr).await.unwrap();
    conn.write_all(&frame_message(&SyncMessage::Hello(hello)).unwrap())
        .await
        .unwrap();

    // The server must reject rather than answer with its own Hello. A clean
    // close (`Ok(None)`) or a reset (`Err`) both count as rejection; only an
    // actual reply would mean the mismatch was accepted.
    let outcome = tokio::time::timeout(OP_TIMEOUT, read_control_frame(&mut *conn))
        .await
        .expect("server must not hang on a mismatched Hello");
    if let Ok(Some(msg)) = outcome {
        panic!("a Hello whose id disagrees with its certificate was answered: {msg:?}");
    }

    server.stop().await;
}

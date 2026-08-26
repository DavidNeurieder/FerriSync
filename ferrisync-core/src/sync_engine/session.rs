use crate::crypto::CryptoProvider;
use crate::domain::SyncOperation;
use crate::domain::folder::FolderId;
use crate::filesystem::SyncRoot;
use crate::protocol_v2::hello::{Hello, HelloFolder};
use crate::protocol::{
    frame_message, Ack, FileChunk, Index, IndexEntry, PairResponse, SyncMessage,
    MAX_CONTROL_FRAME, MAX_FILE_REQUEST_PATHS,
};
use crate::storage::Storage;
use crate::sync::orchestrator::{build_protocol_index, validate_chunk, SyncOrchestrator};
use crate::transport::tcp::TcpTransport;
use crate::transport::TransportConnector;
use crate::DeviceInfo;
use anyhow::Context;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;

/// Window for an inbound peer to complete its TLS handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum time a peer can remain idle after the TLS handshake before the
/// session is torn down.  Prevents resource-exhaustion from many stalled
/// connections.
const SESSION_TIMEOUT: Duration = Duration::from_secs(60);

const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks

/// Outcomes from a sync session.
#[derive(Debug, Default)]
pub struct SyncResult {
    pub pulled: Vec<String>,
    pub pushed: Vec<String>,
    pub conflicts: Vec<String>,
}

/// Run a complete bidirectional sync session as the initiating peer.
#[allow(clippy::too_many_arguments)]
pub async fn run_sync_session(
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    local_path: &str,
    remote_addr: std::net::SocketAddr,
    folder_id: i64,
    device_id: &str,
    event_tx: mpsc::Sender<crate::sync_engine::SyncEvent>,
    state_store: Arc<dyn crate::persistence::StateStore>,
) -> Result<SyncResult> {
    let transport = TcpTransport::new(crypto.clone());
    let mut conn = transport.connect(remote_addr).await.with_context(|| {
        format!(
            "could not reach {remote_addr} — is the peer's app open and \
                 serving this folder? (connect/TLS timed out after {}s)",
            HANDSHAKE_TIMEOUT.as_secs()
        )
    })?;
    let root = Arc::new(SyncRoot::open(PathBuf::from(local_path))?);

    // Refuse to sync with ourselves: a stale row pointing back at this
    // machine (own LAN IP or loopback) would otherwise report a successful
    // no-op against our own server.
    if let Some(peer) = conn.peer_cert_der() {
        let own = crypto.certificate().await;
        let own_bytes = own.to_vec();
        if peer == own_bytes {
            let peer_hash = blake3::hash(&peer);
            let own_hash = blake3::hash(&own_bytes);
            anyhow::bail!(
                "refusing to sync with {remote_addr} — it is this machine \
                 (stale device entry?); peer={peer_hash} own={own_hash} \
                 point the folder at the peer's real address"
            );
        }
    }

    // TOFU: verify peer certificate matches a known paired device
    if let Some(peer) = conn.peer_cert_der() {
        let fingerprint = blake3::hash(&peer);
        if let Some(stored_der) = storage.get_device_cert(device_id)? {
            // Known device — verify fingerprint matches
            if peer != stored_der {
                let stored_fp = blake3::hash(&stored_der);
                anyhow::bail!(
                    "TOFU verification failed for {device_id}: peer presented \
                     cert {fingerprint}, expected {stored_fp} — device cert \
                     may have been regenerated"
                );
            }
        } else {
            // First connection — store the cert (trust on first use)
            storage.set_device_cert(device_id, &peer)?;
        }
    }

    // ── Hello handshake ──
    // Exchange Hello immediately after TLS + TOFU to negotiate protocol
    // version, verify device identity, and advertise folders.
    let device_name = storage.get_device_name(device_id)?.unwrap_or_default();
    let folder_name = storage
        .list_sync_folders()?
        .into_iter()
        .find(|(id, _, _, _, _)| *id == folder_id)
        .map(|(_, path, _, _, _)| path)
        .unwrap_or_default();
    let local_hello = Hello::from_device(
        &DeviceInfo { id: device_id.to_string(), name: device_name, cert_fingerprint: vec![] },
        &crypto,
        vec![HelloFolder {
            id: folder_id.to_string(),
            name: folder_name,
            direction: "both".into(),
        }],
    )
    .await;
    conn.write_all(&frame_message(&SyncMessage::Hello(local_hello.clone()))?)
        .await?;

    let remote_hello_msg = read_message(&mut conn).await?;
    let remote_hello = match remote_hello_msg {
        SyncMessage::Hello(h) => h,
        _ => anyhow::bail!("expected Hello from server"),
    };
    remote_hello.validate().map_err(|e| anyhow::anyhow!("remote Hello invalid: {e}"))?;
    log::info!(
        "Hello from {} ({}) protocol={}",
        remote_hello.device_name,
        remote_hello.device_id,
        remote_hello.protocol_version,
    );

    // Build and send our index
    let local_index = build_protocol_index(PathBuf::from(local_path), folder_id, device_id)?;
    let msg = SyncMessage::Index(Index {
        folder_id: folder_id.to_string(),
        device_id: device_id.to_string(),
        entries: local_index.clone(),
    });
    conn.write_all(&frame_message(&msg)?).await?;

    // Receive remote index
    let remote_index = read_message(&mut conn).await?;
    let remote_index = match remote_index {
        SyncMessage::Index(idx) => idx,
        _ => anyhow::bail!("expected Index message"),
    };

    // Convert both indexes to domain Snapshots and run the pure reconciler
    let folder = FolderId(folder_id);
    let local_snap = SyncOrchestrator::index_to_snapshot(
        &Index { folder_id: folder_id.to_string(), device_id: device_id.to_string(), entries: local_index.clone() },
        folder,
    );
    let remote_snap = SyncOrchestrator::index_to_snapshot(&remote_index, folder);
    let root_for_orch = Arc::new(SyncRoot::open(PathBuf::from(local_path))?);
    let orch = SyncOrchestrator::new(root_for_orch, state_store, folder);
    let plan = orch.reconcile_snapshots(&local_snap, &remote_snap);

    let to_push: Vec<&IndexEntry> = plan.uploads.iter().filter_map(|op| {
        if let SyncOperation::Upload { path, .. } = op {
            local_index.iter().find(|e| e.path == path.0)
        } else { None }
    }).collect();
    let mut to_pull: Vec<&IndexEntry> = plan.downloads.iter().filter_map(|op| {
        if let SyncOperation::Download { path, .. } = op {
            remote_index.entries.iter().find(|e| e.path == path.0)
        } else { None }
    }).collect();
    // Conflicts: pull the remote version (last-writer-wins policy).
    for op in &plan.conflicts {
        let path = op.path();
        if let Some(entry) = remote_index.entries.iter().find(|e| e.path == path.0) {
            to_pull.push(entry);
        }
    }

    let mut result = SyncResult::default();

    // Send files they need (push), then request+receive files we need (pull)
    // We push files first, then send file request, then receive responses & files
    for entry in &to_push {
        let file_path = root.safe_join(&entry.path)?;
        let data = match tokio::fs::read(&file_path).await {
            Ok(d) => d,
            Err(_) => continue,
        };

        send_file_chunks(&mut conn, &entry.path, &data, folder_id).await?;
        result.pushed.push(entry.path.clone());

        let _ = event_tx
            .send(crate::sync_engine::SyncEvent::FilePushed {
                path: entry.path.clone(),
                device: device_id.to_string(),
            })
            .await;
    }

    // No explicit FileRequest: the server proactively pushes everything our
    // index says we lack (it computes the identical set), so requesting
    // again would transfer every pull twice. The sentinel Ack marks the end
    // of our outgoing traffic; the session then drains inbound chunks until
    // all expected pulls have arrived.
    conn.write_all(&frame_message(&SyncMessage::Ack(Ack {
        path: String::new(),
        success: true,
        error: None,
    }))?)
    .await?;

    // Handle incoming messages: FileChunks (files from server), Acks for
    // our pushes, and FileRequests. The session stays open until every
    // pushed file has been acknowledged and every requested file has
    // arrived — closing earlier with unread inbound data makes the kernel
    // send a TCP RST that destroys frames still queued on the peer.
    let mut incoming_files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut buffered_bytes: usize = 0;
    let expected_acks = result.pushed.len();
    let pushed_set: std::collections::HashSet<String> = result.pushed.iter().cloned().collect();
    let mut acked_pushes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let expected_pulls = to_pull.len();
    let mut received_pulls = 0usize;

    loop {
        if acked_pushes.len() >= expected_acks
            && received_pulls >= expected_pulls
            && incoming_files.is_empty()
        {
            break;
        }
        let msg = match tokio::time::timeout(SESSION_TIMEOUT, read_message(&mut conn)).await {
            Ok(Ok(m)) => m,
            Ok(Err(_)) => break,
            Err(_) => {
                log::warn!("client session timed out after {SESSION_TIMEOUT:?}");
                break;
            }
        };

        match msg {
            SyncMessage::FileChunk(chunk) => {
                if chunk.path == "metadata.db" {
                    continue;
                }
                validate_chunk(&chunk, incoming_files.len(), buffered_bytes)?;
                let total = chunk.total_size as usize;
                let entry = incoming_files
                    .entry(chunk.path.clone())
                    .or_insert_with(|| Vec::with_capacity(total));
                let start = chunk.offset as usize;
                let end = start + chunk.data.len();
                if end > entry.len() {
                    entry.resize(end, 0);
                }
                entry[start..end].copy_from_slice(&chunk.data);
                buffered_bytes += chunk.data.len();

                if end >= total {
                    buffered_bytes -= incoming_files[&chunk.path].len();
                    let data = incoming_files.remove(&chunk.path).unwrap();
                    let target = root.safe_join(&chunk.path)?;
                    if let Some(parent) = target.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    if backup_on_conflict(local_path, &chunk.path, &data, &event_tx, device_id)
                        .await?
                    {
                        result.conflicts.push(chunk.path.clone());
                    }
                    tokio::fs::write(&target, &data).await?;
                    log::info!(
                        "pulled {} -> {} ({} bytes)",
                        chunk.path,
                        target.display(),
                        data.len()
                    );

                    conn.write_all(&frame_message(&SyncMessage::Ack(Ack {
                        path: chunk.path.clone(),
                        success: true,
                        error: None,
                    }))?)
                    .await?;

                    result.pulled.push(chunk.path.clone());
                    let _ = event_tx
                        .send(crate::sync_engine::SyncEvent::FilePulled {
                            path: chunk.path,
                            device: device_id.to_string(),
                        })
                        .await;
                    received_pulls += 1;
                }
            }
            SyncMessage::Ack(ack) => {
                if !ack.path.is_empty() && pushed_set.contains(&ack.path) {
                    acked_pushes.insert(ack.path);
                }
            }
            SyncMessage::FileRequest(req) => {
                if req.paths.len() > MAX_FILE_REQUEST_PATHS {
                    log::warn!(
                        "rejecting FileRequest with {} paths (max {MAX_FILE_REQUEST_PATHS})",
                        req.paths.len()
                    );
                    continue;
                }
                for path in &req.paths {
                    if path == "metadata.db" {
                        continue;
                    }
                    let file_path = root.safe_join(path)?;
                    if let Ok(data) = tokio::fs::read(&file_path).await {
                        send_file_chunks(&mut conn, path, &data, folder_id).await?;
                    }
                }
            }
            _ => {}
        }
    }

    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), async {
        loop {
            if read_message(&mut conn).await.is_err() {
                break;
            }
        }
    })
    .await;
    let _ = conn.close().await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let _ = storage.set_folder_last_sync(folder_id, now);

    let pushed_list = result.pushed.join(", ");
    let pulled_list = result.pulled.join(", ");
    log::info!(
        "session with {remote_addr} done: pushed {} ({pushed_list}), pulled {} ({pulled_list}), conflicts {}",
        result.pushed.len(),
        result.pulled.len(),
        result.conflicts.len()
    );
    let _ = storage.record_session(
        "outgoing",
        device_id,
        &remote_addr.to_string(),
        local_path,
        result.pushed.len(),
        result.pulled.len(),
        result.conflicts.len(),
    );

    Ok(result)
}

/// Listen for incoming connections as a server.
/// Accepts both pairing requests and sync sessions on the same port.
///
/// The accept loop runs in a spawned task; the returned handle can be used to
/// await (or abort) it. The loop exits when `shutdown` fires or the sender is
/// dropped. The bound socket address is also returned so callers that pass
/// port 0 learn which port the OS picked.
#[allow(clippy::too_many_arguments)]
pub async fn listen_for_sync(
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    addr: std::net::SocketAddr,
    local_path: String,
    folder_id: i64,
    event_tx: mpsc::Sender<crate::sync_engine::SyncEvent>,
    device_info: DeviceInfo,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    gate: crate::sync_engine::server::PairGate,
    state_store: Arc<dyn crate::persistence::StateStore>,
) -> Result<(tokio::task::JoinHandle<()>, std::net::SocketAddr)> {
    let listener = TcpListener::bind(addr).await?;
    let bound_addr = listener.local_addr()?;

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                accepted = listener.accept() => {
                    match accepted {
                        Ok((tcp, _)) => {
                    let crypto = crypto.clone();
                    let storage = storage.clone();
                    let local_path = local_path.clone();
                    let event_tx = event_tx.clone();
                    let device_info = device_info.clone();
                    let gate = gate.clone();
                    let state_store = state_store.clone();

                    tokio::spawn(async move {
                        let config = match crypto.server_config().await {
                            Ok(c) => c,
                            Err(e) => {
                                log::error!("TLS config failed: {e}");
                                return;
                            }
                        };
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
                        let peer_addr = tcp.peer_addr().ok();
                        let mut tls = match timeout(HANDSHAKE_TIMEOUT, acceptor.accept(tcp)).await {
                            Ok(Ok(t)) => tokio_rustls::TlsStream::Server(t),
                            Ok(Err(e)) => {
                                log::error!("TLS accept failed: {e}");
                                return;
                            }
                            Err(_) => {
                                log::warn!("peer did not complete TLS handshake in time");
                                return;
                            }
                        };

                        // ── Read first message ──
                        // Expect Hello as the new handshake; PairRequest/Index
                        // are accepted for backward compatibility.
                        let first_msg = match read_tls_message(&mut tls).await {
                            Ok(m) => m,
                            Err(e) => {
                                log::error!("failed to read initial message: {e}");
                                return;
                            }
                        };

                        // If the first message is Hello, exchange and read the
                        // real message that follows.
                        let msg = match first_msg {
                            SyncMessage::Hello(remote_hello) => {
                                if let Err(e) = remote_hello.validate() {
                                    log::warn!("invalid Hello from peer: {e}");
                                    return;
                                }
                                log::info!(
                                    "Hello from {} ({}) protocol={}",
                                    remote_hello.device_name,
                                    remote_hello.device_id,
                                    remote_hello.protocol_version,
                                );
                                let folder_name = storage
                                    .list_sync_folders()
                                    .ok()
                                    .and_then(|folders| {
                                        folders.into_iter().find(|(id, _, _, _, _)| *id == folder_id).map(|(_, path, _, _, _)| path)
                                    })
                                    .unwrap_or_default();
                                let local_hello = Hello::from_device(
                                    &device_info,
                                    &crypto,
                                    vec![HelloFolder {
                                        id: folder_id.to_string(),
                                        name: folder_name,
                                        direction: "both".into(),
                                    }],
                                )
                                .await;
                                if let Ok(framed) = frame_message(&SyncMessage::Hello(local_hello)) {
                                    let _ = tls.write_all(&framed).await;
                                }
                                // Read the real message after Hello exchange
                                match read_tls_message(&mut tls).await {
                                    Ok(m) => Ok(m),
                                    Err(e) => {
                                        log::error!("failed to read message after Hello: {e}");
                                        Err(e)
                                    }
                                }
                            }
                            other => Ok(other),
                        };

                        match msg {
                            Ok(SyncMessage::PairRequest(req)) => {
                                log::info!(
                                    "Pair request from {} ({})",
                                    req.device_name,
                                    req.device_id
                                );
                                use crate::sync_engine::server::{Admission, DENIED_REASON, PENDING_REASON};
                                let (accepted, reason) = match gate.admit(&req.device_id, &req.device_name).await {
                                    Admission::Accept => (true, None),
                                    Admission::Hold => (false, Some(PENDING_REASON.to_string())),
                                    Admission::Deny => (false, Some(DENIED_REASON.to_string())),
                                };
                                if accepted {
                                    log::info!("Pairing accepted for {} ({})", req.device_name, req.device_id);
                                }
                                let resp = SyncMessage::PairResponse(PairResponse {
                                    accepted,
                                    device_id: device_info.id.clone(),
                                    device_name: device_info.name.clone(),
                                    cert_fingerprint: device_info.cert_fingerprint.clone(),
                                    reason,
                                });
                                if let Ok(framed) = frame_message(&resp) {
                                    let _ = tls.write_all(&framed).await;
                                }
                                if accepted {
                                    // Extract peer certificate for TOFU pinning
                                    let peer_cert = tls.get_ref().1
                                        .peer_certificates()
                                        .and_then(|c| c.first())
                                        .map(|c| c.as_ref().to_vec());
                                    if let Err(e) =
                                        storage.upsert_device(
                                            &req.device_id,
                                            &req.device_name,
                                            peer_cert.as_deref(),
                                            None,
                                        )
                                    {
                                        log::error!("failed to store paired device: {e}");
                                    } else {
                                        gate.paired(&req.device_name, &req.device_id).await;
                                    }
                                    // Record where the pairing came from so the
                                    // host can dial the peer without waiting for
                                    // mDNS. The port is our default, not
                                    // necessarily their listener; successful syncs
                                    // refine it later.
                                    if let Some(peer) = peer_addr {
                                        // Dual-stack listeners report IPv4
                                        // peers as ::ffff:a.b.c.d; store the
                                        // plain form so it stays dialable.
                                        let ip = match peer.ip() {
                                            std::net::IpAddr::V6(v6) => v6
                                                .to_ipv4_mapped()
                                                .map(std::net::IpAddr::V4)
                                                .unwrap_or_else(|| peer.ip()),
                                            other => other,
                                        };
                                        let recorded =
                                            format!("{}:{}", ip, crate::sync_engine::bulk::DEFAULT_PORT);
                                        match storage.set_device_last_addr(
                                            &req.device_id,
                                            &recorded,
                                        ) {
                                            Ok(()) => log::info!(
                                                "Recorded {recorded} for {}",
                                                req.device_name
                                            ),
                                            Err(e) => log::warn!(
                                                "could not record peer address: {e}"
                                            ),
                                        }
                                    }
                                }
                            }
                            Ok(SyncMessage::Index(idx)) => {
                                // TOFU: authenticate peer before syncing
                                let peer_cert = tls.get_ref().1
                                    .peer_certificates()
                                    .and_then(|c| c.first())
                                    .map(|c| c.as_ref().to_vec());
                                match peer_cert {
                                    Some(cert) => {
                                        let fingerprint = blake3::hash(&cert);
                                        match storage.get_device_by_cert_fingerprint(fingerprint.as_bytes()) {
                                            Ok(Some(peer_device)) => {
                                                log::info!(
                                                    "sync session from authenticated device {peer_device}"
                                                );
                                                if let Err(e) = handle_server_session(
                                                    &mut tls,
                                                    crypto,
                                                    storage,
                                                    &local_path,
                                                    folder_id,
                                                    event_tx,
                                                    idx,
                                                    &device_info.id,
                                                    state_store,
                                                )
                                                .await
                                                {
                                                    log::error!("session error: {e}");
                                                }
                                            }
                                            Ok(None) => {
                                                log::warn!(
                                                    "rejecting sync from unpaired device (cert={fingerprint})"
                                                );
                                            }
                                            Err(e) => {
                                                log::error!("cert lookup failed: {e}");
                                            }
                                        }
                                    }
                                    None => {
                                        log::warn!(
                                            "rejecting sync: peer did not present a TLS certificate"
                                        );
                                    }
                                }
                            }
                            Ok(_) => {
                                log::warn!("unexpected message type from incoming connection");
                            }
                            Err(e) => {
                                log::error!(
                                    "failed to read initial message from {}: {e}",
                                    peer_addr
                                        .map(|a| a.to_string())
                                        .unwrap_or_else(|| "unknown peer".to_string())
                                );
                            }
                        }
                    });
                }
                Err(e) => {
                    log::error!("accept error: {e}");
                }
                }
                }
            }
        }
        log::info!("server listener on {addr} stopped");
    });

    Ok((handle, bound_addr))
}

/// Read first message from a TLS stream and dispatch to `handle_server_session`.
/// Convenience wrapper used by tests.
#[allow(clippy::too_many_arguments)]
pub async fn handle_server_session_with_read(
    conn: &mut tokio_rustls::TlsStream<tokio::net::TcpStream>,
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    local_path: &str,
    folder_id: i64,
    event_tx: mpsc::Sender<crate::sync_engine::SyncEvent>,
    device_id: &str,
    state_store: Arc<dyn crate::persistence::StateStore>,
) -> Result<()> {
    let remote_msg = read_tls_message(conn).await?;
    // Handle Hello if present, then read Index
    let remote_index = match remote_msg {
        SyncMessage::Hello(remote_hello) => {
            if let Err(e) = remote_hello.validate() {
                anyhow::bail!("invalid Hello: {e}");
            }
            // Respond with our Hello
            let device_name = storage.get_device_name(device_id)?.unwrap_or_default();
            let local_hello = Hello::from_device(
                &DeviceInfo { id: device_id.to_string(), name: device_name, cert_fingerprint: vec![] },
                &crypto,
                vec![],
            )
            .await;
            if let Ok(framed) = frame_message(&SyncMessage::Hello(local_hello)) {
                let _ = conn.write_all(&framed).await;
            }
            // Read the Index that follows
            let next_msg = read_tls_message(conn).await?;
            match next_msg {
                SyncMessage::Index(idx) => idx,
                _ => anyhow::bail!("expected Index after Hello"),
            }
        }
        SyncMessage::Index(idx) => idx,
        _ => anyhow::bail!("expected Hello or Index"),
    };
    handle_server_session(
        conn,
        crypto,
        storage,
        local_path,
        folder_id,
        event_tx,
        remote_index,
        device_id,
        state_store,
    )
    .await
}

/// Handle a sync session from the server side.
#[allow(clippy::too_many_arguments)]
pub async fn handle_server_session(
    conn: &mut tokio_rustls::TlsStream<tokio::net::TcpStream>,
    _crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    local_path: &str,
    folder_id: i64,
    event_tx: mpsc::Sender<crate::sync_engine::SyncEvent>,
    remote_index: Index,
    device_id: &str,
    state_store: Arc<dyn crate::persistence::StateStore>,
) -> Result<()> {
    // Build and send our index
    let local_entries = build_protocol_index(PathBuf::from(local_path), folder_id, device_id)?;
    let msg = SyncMessage::Index(Index {
        folder_id: folder_id.to_string(),
        device_id: device_id.to_string(),
        entries: local_entries.clone(),
    });
    conn.write_all(&frame_message(&msg)?).await?;
    let root = Arc::new(SyncRoot::open(PathBuf::from(local_path))?);

    // Convert both indexes to domain Snapshots and run the pure reconciler
    let folder = FolderId(folder_id);
    let local_snap = SyncOrchestrator::index_to_snapshot(
        &Index { folder_id: folder_id.to_string(), device_id: device_id.to_string(), entries: local_entries.clone() },
        folder,
    );
    let remote_snap = SyncOrchestrator::index_to_snapshot(&remote_index, folder);
    let orch = SyncOrchestrator::new(root.clone(), state_store, folder);
    let plan = orch.reconcile_snapshots(&local_snap, &remote_snap);

    // Compute what they need from us so we can push it proactively
    let mut to_push_to_client: Vec<&IndexEntry> = plan.uploads.iter().filter_map(|op| {
        if let SyncOperation::Upload { path, .. } = op {
            local_entries.iter().find(|e| e.path == path.0)
        } else { None }
    }).collect();
    // Conflicts: also push our local version (last-writer-wins policy).
    for op in &plan.conflicts {
        let path = op.path();
        if let Some(entry) = local_entries.iter().find(|e| e.path == path.0) {
            to_push_to_client.push(entry);
        }
    }

    // Push our files to client
    for entry in &to_push_to_client {
        let file_path = root.safe_join(&entry.path)?;
        let data = match tokio::fs::read(&file_path).await {
            Ok(d) => d,
            Err(_) => continue,
        };
        send_file_chunks_tls(conn, &entry.path, &data, folder_id).await?;
        let _ = event_tx
            .send(crate::sync_engine::SyncEvent::FilePushed {
                path: entry.path.clone(),
                device: "remote".to_string(),
            })
            .await;
    }

    // Sentinel: signals we are done pushing
    conn.write_all(&frame_message(&SyncMessage::Ack(Ack {
        path: String::new(),
        success: true,
        error: None,
    }))?)
    .await?;

    // Handle all incoming messages: FileChunks (pushed files from client),
    // FileRequests (client requesting more files from us), Acks
    let mut incoming_files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut buffered_bytes: usize = 0;
    let mut got_eof = false;
    let mut received_pushes: Vec<String> = Vec::new();
    let mut served_requests: Vec<String> = Vec::new();
    let mut conflicts = 0usize;

    loop {
        let msg = match tokio::time::timeout(SESSION_TIMEOUT, read_tls_message(conn)).await {
            Ok(Ok(m)) => m,
            Ok(Err(_)) => break,
            Err(_) => {
                log::warn!("server session timed out after {SESSION_TIMEOUT:?}");
                break;
            }
        };

        match msg {
            SyncMessage::FileChunk(chunk) => {
                if chunk.path == "metadata.db" {
                    continue;
                }
                validate_chunk(&chunk, incoming_files.len(), buffered_bytes)?;
                let total = chunk.total_size as usize;
                let entry = incoming_files
                    .entry(chunk.path.clone())
                    .or_insert_with(|| Vec::with_capacity(total));
                let start = chunk.offset as usize;
                let end = start + chunk.data.len();
                if end > entry.len() {
                    entry.resize(end, 0);
                }
                entry[start..end].copy_from_slice(&chunk.data);
                buffered_bytes += chunk.data.len();

                if end >= total {
                    buffered_bytes -= incoming_files[&chunk.path].len();
                    let data = incoming_files.remove(&chunk.path).unwrap();
                    let target = root.safe_join(&chunk.path)?;
                    if let Some(parent) = target.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    if backup_on_conflict(local_path, &chunk.path, &data, &event_tx, "remote")
                        .await?
                    {
                        conflicts += 1;
                    }
                    tokio::fs::write(&target, &data).await?;
                    received_pushes.push(chunk.path.clone());

                    conn.write_all(&frame_message(&SyncMessage::Ack(Ack {
                        path: chunk.path.clone(),
                        success: true,
                        error: None,
                    }))?)
                    .await?;

                    let _ = event_tx
                        .send(crate::sync_engine::SyncEvent::FilePulled {
                            path: chunk.path,
                            device: "remote".to_string(),
                        })
                        .await;
                }
            }
            SyncMessage::FileRequest(req) => {
                if req.paths.len() > MAX_FILE_REQUEST_PATHS {
                    log::warn!(
                        "rejecting FileRequest with {} paths (max {MAX_FILE_REQUEST_PATHS})",
                        req.paths.len()
                    );
                    continue;
                }
                // Client requests files from us
                for path in &req.paths {
                    if path == "metadata.db" {
                        continue;
                    }
                    let file_path = root.safe_join(path)?;
                    if let Ok(data) = tokio::fs::read(&file_path).await {
                        send_file_chunks_tls(conn, path, &data, folder_id).await?;
                        served_requests.push(path.clone());
                    }
                }
            }
            SyncMessage::Ack(ack) => {
                if ack.path.is_empty() {
                    got_eof = true;
                }
            }
            _ => {}
        }

        if got_eof && incoming_files.is_empty() {
            break;
        }
    }

    // Drain any remaining messages briefly
    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), async {
        loop {
            if read_tls_message(conn).await.is_err() {
                break;
            }
        }
    })
    .await;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let _ = storage.set_folder_last_sync(folder_id, now);

    let sent_list = to_push_to_client
        .iter()
        .map(|e| e.path.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let served_list = served_requests.join(", ");
    let received_list = received_pushes.join(", ");
    log::info!(
        "served session for {local_path}: sent {} ({sent_list}), answered requests for {} ({served_list}), received {} ({received_list}), conflicts {}",
        to_push_to_client.len(),
        served_requests.len(),
        received_pushes.len(),
        conflicts
    );
    let peer_label = conn
        .get_ref()
        .0
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let _ = storage.record_session(
        "incoming",
        &peer_label,
        &peer_label,
        local_path,
        received_pushes.len(),
        to_push_to_client.len() + served_requests.len(),
        conflicts,
    );

    Ok(())
}

/// Send a file as framed chunks over a transport connection.
async fn send_file_chunks(
    conn: &mut Box<dyn crate::transport::TransportConnection>,
    path: &str,
    data: &[u8],
    folder_id: i64,
) -> Result<()> {
    let total_size = data.len() as u64;
    let mut offset = 0u64;
    loop {
        let end = (offset as usize + CHUNK_SIZE).min(total_size as usize);
        let chunk = FileChunk {
            folder_id: folder_id.to_string(),
            path: path.to_string(),
            offset,
            data: data[offset as usize..end].to_vec(),
            total_size,
        };
        conn.write_all(&frame_message(&SyncMessage::FileChunk(chunk))?)
            .await?;
        offset = end as u64;
        if offset >= total_size {
            break;
        }
    }
    Ok(())
}

/// Send a file as framed chunks over a TLS stream.
async fn send_file_chunks_tls(
    conn: &mut tokio_rustls::TlsStream<tokio::net::TcpStream>,
    path: &str,
    data: &[u8],
    folder_id: i64,
) -> Result<()> {
    let total_size = data.len() as u64;
    let mut offset = 0u64;
    loop {
        let end = (offset as usize + CHUNK_SIZE).min(total_size as usize);
        let chunk = FileChunk {
            folder_id: folder_id.to_string(),
            path: path.to_string(),
            offset,
            data: data[offset as usize..end].to_vec(),
            total_size,
        };
        conn.write_all(&frame_message(&SyncMessage::FileChunk(chunk))?)
            .await?;
        offset = end as u64;
        if offset >= total_size {
            break;
        }
    }
    Ok(())
}

/// If the file exists with different content, back it up and emit a Conflict event.
/// Returns `true` if a conflict was detected and backed up.
async fn backup_on_conflict(
    local_path: &str,
    path: &str,
    incoming_data: &[u8],
    event_tx: &mpsc::Sender<crate::sync_engine::SyncEvent>,
    winner_label: &str,
) -> Result<bool> {
    let root = SyncRoot::open(PathBuf::from(local_path))?;
    let target = root.safe_join(path)?;
    if !target.exists() {
        return Ok(false);
    }
    let existing = tokio::fs::read(&target).await?;
    if existing == incoming_data {
        return Ok(false);
    }
    // Content differs — rename to a unique conflict file before overwriting
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let short_hash = &blake3::hash(incoming_data).to_hex()[..8];
    let loser_label = if winner_label == "remote" {
        "local"
    } else {
        "remote"
    };
    let bak = PathBuf::from(format!(
        "{}.ferrisync-conflict-{ts}-{loser_label}-{short_hash}",
        target.display()
    ));
    tokio::fs::rename(&target, &bak).await?;
    let _ = event_tx
        .send(crate::sync_engine::SyncEvent::Conflict {
            path: path.to_string(),
            winner: winner_label.to_string(),
            loser: loser_label.to_string(),
        })
        .await;
    Ok(true)
}

// ── I/O helpers ──

async fn read_exact(
    conn: &mut Box<dyn crate::transport::TransportConnection>,
    mut buf: &mut [u8],
) -> Result<()> {
    while !buf.is_empty() {
        let n = conn.read(buf).await?;
        if n == 0 {
            anyhow::bail!("connection closed");
        }
        buf = &mut buf[n..];
    }
    Ok(())
}

async fn read_message(
    conn: &mut Box<dyn crate::transport::TransportConnection>,
) -> Result<SyncMessage> {
    let mut len_buf = [0u8; 4];
    read_exact(conn, &mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_CONTROL_FRAME {
        anyhow::bail!("frame too large: {len} bytes (max {MAX_CONTROL_FRAME})");
    }
    let mut payload = vec![0u8; len];
    read_exact(conn, &mut payload).await?;
    Ok(bincode::deserialize(&payload)?)
}

async fn read_tls_message(
    conn: &mut tokio_rustls::TlsStream<tokio::net::TcpStream>,
) -> Result<SyncMessage> {
    let mut len_buf = [0u8; 4];
    conn.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_CONTROL_FRAME {
        anyhow::bail!("frame too large: {len} bytes (max {MAX_CONTROL_FRAME})");
    }
    let mut payload = vec![0u8; len];
    conn.read_exact(&mut payload).await?;
    Ok(bincode::deserialize(&payload)?)
}

use crate::crypto::CryptoProvider;
use crate::domain::folder::FolderId;
use crate::domain::SyncOperation;
use crate::filesystem::SyncRoot;
use crate::protocol::{
    frame_message, Ack, FileChunk, Index, IndexEntry, PairResponse, SyncMessage, MAX_CONTROL_FRAME,
    MAX_FILE_REQUEST_PATHS, MAX_PATH_LEN,
};
use crate::protocol_v2::hello::{Hello, HelloFolder};
use crate::protocol_v2::shared::SharedFolderInfo;
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

/// Maximum number of concurrent inbound connections (TLS handshakes or
/// sessions) the accept loop will process at once.  Beyond this, new
/// connections are shed immediately to prevent resource exhaustion.
const MAX_SESSIONS: usize = 64;

/// Outcomes from a sync session.
#[derive(Debug, Default)]
pub struct SyncResult {
    pub pulled: Vec<String>,
    pub pushed: Vec<String>,
    pub conflicts: Vec<String>,
    /// Total bytes received from the peer during this session.
    pub pulled_bytes: u64,
    /// Total bytes sent to the peer during this session.
    pub pushed_bytes: u64,
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
    dry_run: bool,
    remote_path: Option<&str>,
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
    //
    // The client MUST present its OWN identity in its Hello, and that identity
    // is derived from its TLS certificate (not the self-claimed id the caller
    // used to address the peer). The server rejects any Hello whose client id
    // does not match the certificate it presented.
    let my_id = crate::crypto::cert_to_device_id(&crypto.certificate().await);
    let device_name = storage.get_device_name(&my_id)?.unwrap_or_default();
    let folder_name = storage
        .list_sync_folders()?
        .into_iter()
        .find(|(id, _, _, _, _)| *id == folder_id)
        .map(|(_, path, _, _, _)| path)
        .unwrap_or_default();
    let local_hello = Hello::from_device(
        &DeviceInfo {
            id: my_id.clone(),
            name: device_name,
            cert_fingerprint: vec![],
        },
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
    remote_hello
        .validate()
        .map_err(|e| anyhow::anyhow!("remote Hello invalid: {e}"))?;
    // Verify the server's claimed device_id matches its TLS certificate.
    let peer_cert = conn.peer_cert_der();
    if let Some(expected) = peer_cert.as_deref().map(crate::crypto::cert_to_device_id) {
        if remote_hello.device_id != expected {
            anyhow::bail!(
                "identity mismatch: server Hello claims device_id {} but its \
                 TLS certificate maps to {} — refusing session",
                remote_hello.device_id,
                expected
            );
        }
    }
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
        remote_path: remote_path.map(|p| p.to_string()),
        entries: local_index.clone(),
    });
    conn.write_all(&frame_message(&msg)?).await?;

    // Receive remote index
    let remote_index = read_message(&mut conn).await?;
    let remote_index = match remote_index {
        SyncMessage::Index(idx) => {
            if idx.entries.len() > crate::protocol::MAX_INDEX_ENTRIES {
                anyhow::bail!(
                    "remote index too large: {} entries (max {})",
                    idx.entries.len(),
                    crate::protocol::MAX_INDEX_ENTRIES
                );
            }
            idx
        }
        _ => anyhow::bail!("expected Index message"),
    };

    // Note: we deliberately do NOT compare the remote's folder_id against
    // our local one. folder_id is a per-device local storage key, not a
    // shared capability token — the same logical folder carries a different
    // row id on each side, so cross-peer equality would wrongly reject
    // legitimate syncs. Folders are authorized by the TLS certificate of the
    // paired peer, not by a peer-supplied id.

    // Convert both indexes to domain Snapshots and run the pure reconciler
    let folder = FolderId(folder_id);
    let local_snap = SyncOrchestrator::index_to_snapshot(
        &Index {
            folder_id: folder_id.to_string(),
            device_id: device_id.to_string(),
            remote_path: remote_path.map(|p| p.to_string()),
            entries: local_index.clone(),
        },
        folder,
    );
    let remote_snap = SyncOrchestrator::index_to_snapshot(&remote_index, folder);
    let root_for_orch = Arc::new(SyncRoot::open(PathBuf::from(local_path))?);
    let orch = SyncOrchestrator::new(root_for_orch, state_store, folder);
    let plan = orch.reconcile_snapshots(&local_snap, &remote_snap);

    let to_push: Vec<&IndexEntry> = plan
        .uploads
        .iter()
        .filter_map(|op| {
            if let SyncOperation::Upload { path, .. } = op {
                local_index.iter().find(|e| e.path == path.0)
            } else {
                None
            }
        })
        .collect();
    let mut to_pull: Vec<&IndexEntry> = plan
        .downloads
        .iter()
        .filter_map(|op| {
            if let SyncOperation::Download { path, .. } = op {
                remote_index.entries.iter().find(|e| e.path == path.0)
            } else {
                None
            }
        })
        .collect();
    // Conflicts: pull the remote version (last-writer-wins policy).
    for op in &plan.conflicts {
        let path = op.path();
        if let Some(entry) = remote_index.entries.iter().find(|e| e.path == path.0) {
            to_pull.push(entry);
        }
    }

    let mut result = SyncResult::default();

    // Live progress: totals come from the reconciled transfer plan (files the
    // peer has that we lack, plus files we have that the peer lacks), so a
    // percentage computed from done/total is honest for this session.
    let total_files = (to_push.len() + to_pull.len()) as u64;
    let total_bytes: u64 = to_push.iter().chain(to_pull.iter()).map(|e| e.size).sum();
    let mut done_files = 0u64;
    let mut done_bytes = 0u64;
    emit_progress(
        &event_tx,
        folder_id,
        "starting",
        done_files,
        done_bytes,
        total_files,
        total_bytes,
    );

    // Dry-run: report the reconciliation plan without transferring anything.
    if dry_run {
        let mut result = SyncResult::default();
        result.pushed = to_push.iter().map(|e| e.path.clone()).collect();
        result.pulled = to_pull.iter().map(|e| e.path.clone()).collect();
        result.pushed_bytes = to_push.iter().map(|e| e.size).sum();
        result.pulled_bytes = to_pull.iter().map(|e| e.size).sum();
        let mut conflict_paths = Vec::new();
        for op in &plan.conflicts {
            conflict_paths.push(op.path().0.clone());
        }
        result.conflicts = conflict_paths;
        emit_progress(
            &event_tx,
            folder_id,
            "done",
            total_files,
            total_bytes,
            total_files,
            total_bytes,
        );
        return Ok(result);
    }

    // Build expected hashes from the remote index for integrity verification.
    let expected_hashes: HashMap<String, Vec<u8>> = remote_index
        .entries
        .iter()
        .map(|e| (e.path.clone(), e.hash.clone()))
        .collect();

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
        result.pushed_bytes += data.len() as u64;
        done_files += 1;
        done_bytes += data.len() as u64;
        emit_progress(
            &event_tx,
            folder_id,
            "uploading",
            done_files,
            done_bytes,
            total_files,
            total_bytes,
        );

        record_history_row(
            &storage,
            folder_id,
            &entry.path,
            device_id,
            "push",
            entry.mtime,
            entry.size as i64,
            &entry.hash,
        );

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

                    // Verify file integrity before committing to disk.
                    let actual_hash = blake3::hash(&data).as_bytes().to_vec();
                    if let Some(expected) = expected_hashes.get(&chunk.path) {
                        if &actual_hash != expected {
                            log::warn!("hash mismatch for {} — rejecting download", chunk.path,);
                            conn.write_all(&frame_message(&SyncMessage::Ack(Ack {
                                path: chunk.path.clone(),
                                success: false,
                                error: Some("hash mismatch".into()),
                            }))?)
                            .await?;
                            continue;
                        }
                    }

                    let target = root.safe_join(&chunk.path)?;
                    if let Some(parent) = target.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    let conflicted =
                        backup_on_conflict(local_path, &chunk.path, &data, &event_tx, device_id)
                            .await?;
                    if conflicted {
                        result.conflicts.push(chunk.path.clone());
                    }
                    // Atomic write: temp file + rename to prevent corruption on crash.
                    let temp_path = target.with_extension(".ferrisync-tmp");
                    tokio::fs::write(&temp_path, &data).await?;
                    tokio::fs::rename(&temp_path, &target).await.map_err(|e| {
                        let _ = std::fs::remove_file(&temp_path);
                        anyhow::anyhow!("atomic rename failed: {e}")
                    })?;
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
                    result.pulled_bytes += data.len() as u64;
                    done_files += 1;
                    done_bytes += data.len() as u64;
                    emit_progress(
                        &event_tx,
                        folder_id,
                        "downloading",
                        done_files,
                        done_bytes,
                        total_files,
                        total_bytes,
                    );
                    record_history_row(
                        &storage,
                        folder_id,
                        &chunk.path,
                        device_id,
                        if conflicted { "conflict" } else { "pull" },
                        file_mtime_secs(&target),
                        data.len() as i64,
                        &actual_hash,
                    );
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
                    if path == "metadata.db" || path.len() > MAX_PATH_LEN {
                        continue;
                    }
                    let file_path = root.safe_join(path)?;
                    if let Ok(data) = tokio::fs::read(&file_path).await {
                        send_file_chunks(&mut conn, path, &data, folder_id).await?;
                        result.pushed_bytes += data.len() as u64;
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
        result.pushed_bytes,
        result.pulled_bytes,
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

    // Bound the number of concurrent connections to defend against resource
    // exhaustion from many simultaneous handshakes/sessions.
    let concurrency = Arc::new(tokio::sync::Semaphore::new(MAX_SESSIONS));

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
                    let concurrency = concurrency.clone();

                    tokio::spawn(async move {
                        // If too many sessions are already in flight, shed
                        // this connection immediately rather than spending
                        // resources on it. The permit guard is held for this
                        // task's whole lifetime, bounding concurrent sessions.
                        let Ok(permit) = concurrency.try_acquire() else {
                            log::warn!("connection rejected: too many concurrent sessions");
                            return;
                        };
                        let _session_quota = permit;
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
                                // The Hello's claimed device_id MUST equal the
                                // identity derived from the TLS certificate,
                                // otherwise the transport and application
                                // identities diverge.
                                let peer_cert = tls.get_ref().1
                                    .peer_certificates()
                                    .and_then(|c| c.first())
                                    .map(|c| c.as_ref().to_vec());
                                let derived = peer_cert
                                    .as_deref()
                                    .map(crate::crypto::cert_to_device_id);
                                if let Some(expected) = derived {
                                    if remote_hello.device_id != expected {
                                        log::warn!(
                                            "rejecting Hello: claimed device_id {} != TLS-derived {}",
                                            remote_hello.device_id,
                                            expected
                                        );
                                        return;
                                    }
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
                                // Advertise our cert-derived identity in the
                                // Hello so the peer's Hello check matches.
                                let hello_info = DeviceInfo {
                                    id: crate::crypto::cert_to_device_id(&crypto.certificate().await),
                                    name: device_info.name.clone(),
                                    cert_fingerprint: device_info.cert_fingerprint.clone(),
                                };
                                let local_hello = Hello::from_device(
                                    &hello_info,
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
                                let peer_cert = tls.get_ref().1
                                    .peer_certificates()
                                    .and_then(|c| c.first())
                                    .map(|c| c.as_ref().to_vec());
                                // Derive the peer's authoritative identity from
                                // its TLS certificate, NOT from the self-claimed
                                // device_id in the request. This prevents a
                                // peer from impersonating a known device by
                                // spoofing its id.
                                let derived_id = peer_cert
                                    .as_deref()
                                    .map(crate::crypto::cert_to_device_id)
                                    .unwrap_or_else(|| req.device_id.clone());
                                let (accepted, reason) = match gate.admit(&derived_id, &req.device_name, peer_cert.clone()).await {
                                    Admission::Accept => (true, None),
                                    Admission::Hold => (false, Some(PENDING_REASON.to_string())),
                                    Admission::Deny => (false, Some(DENIED_REASON.to_string())),
                                };
                                if accepted {
                                    log::info!("Pairing accepted for {} ({})", req.device_name, derived_id);
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
                                    if let Err(e) =
                                        storage.upsert_device(
                                            &derived_id,
                                            &req.device_name,
                                            peer_cert.as_deref(),
                                            None,
                                        )
                                    {
                                        log::error!("failed to store paired device: {e}");
                                    } else {
                                        gate.paired(&req.device_name, &derived_id).await;
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
                                            &derived_id,
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
                            Ok(SyncMessage::ListSharedFolders) => {
                                // Post-auth RPC: reveal the serving device's
                                // discoverable shared folders to an
                                // authenticated peer (never via mDNS).
                                let folders: Vec<SharedFolderInfo> = storage
                                    .list_shared_folders(&device_info.id)
                                    .unwrap_or_default()
                                    .into_iter()
                                    .filter(|r| r.5 && r.6) // discoverable && enabled
                                    .map(|r| SharedFolderInfo::new(&r.1, &r.3, "both", &r.4))
                                    .collect();
                                if let Ok(framed) =
                                    frame_message(&SyncMessage::SharedFolders(folders))
                                {
                                    let _ = tls.write_all(&framed).await;
                                }
                            }
                            Ok(SyncMessage::RequestFolderPair(req)) => {
                                // The peer's authoritative identity comes from
                                // its TLS certificate, not the self-claimed id.
                                let peer_cert = tls.get_ref().1
                                    .peer_certificates()
                                    .and_then(|c| c.first())
                                    .map(|c| c.as_ref().to_vec());
                                let mut req = req;
                                if let Some(cert) = peer_cert.as_deref() {
                                    req.device_id =
                                        crate::crypto::cert_to_device_id(cert).to_string();
                                }
                                // Only ever pair to a share this device actually
                                // exposes and that is discoverable.
                                let share_ok = storage
                                    .shared_folder_by_guid(&device_info.id, &req.folder_guid)
                                    .ok()
                                    .flatten()
                                    .map(|r| r.5 && r.6)
                                    .unwrap_or(false);
                                if !share_ok {
                                    if let Ok(framed) = frame_message(&SyncMessage::FolderPairRejected(
                                        "unknown or not shared folder".to_string(),
                                    )) {
                                        let _ = tls.write_all(&framed).await;
                                    }
                                } else {
                                    use crate::sync_engine::server::FolderPairOutcome;
                                    // Capture the owner-facing event fields before
                                    // moving `req` into the gate.
                                    let (ev_name, ev_id, ev_folder_guid) = (
                                        req.device_name.clone(),
                                        req.device_id.clone(),
                                        req.folder_guid.clone(),
                                    );
                                    match gate.request_folder_pair(req) {
                                        FolderPairOutcome::Approved(grant) => {
                                            if let Ok(framed) =
                                                frame_message(&SyncMessage::FolderPairApproved(grant))
                                            {
                                                let _ = tls.write_all(&framed).await;
                                            }
                                        }
                                        FolderPairOutcome::Rejected => {
                                            if let Ok(framed) = frame_message(
                                                &SyncMessage::FolderPairRejected(
                                                    "denied".to_string(),
                                                ),
                                            ) {
                                                let _ = tls.write_all(&framed).await;
                                            }
                                        }
                                        FolderPairOutcome::Pending => {
                                            let _ = event_tx
                                                .send(crate::sync_engine::SyncEvent::FolderPairRequested {
                                                    name: ev_name,
                                                    id: ev_id,
                                                    folder: ev_folder_guid,
                                                })
                                                .await;
                                            if let Ok(framed) =
                                                frame_message(&SyncMessage::FolderPairPending)
                                            {
                                                let _ = tls.write_all(&framed).await;
                                            }
                                        }
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
            // Respond with our Hello — advertising our cert-derived identity.
            let my_id = crate::crypto::cert_to_device_id(&crypto.certificate().await);
            let device_name = storage.get_device_name(&my_id)?.unwrap_or_default();
            let local_hello = Hello::from_device(
                &DeviceInfo {
                    id: my_id,
                    name: device_name,
                    cert_fingerprint: vec![],
                },
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
    // Reject oversized remote indexes before processing.
    if remote_index.entries.len() > crate::protocol::MAX_INDEX_ENTRIES {
        anyhow::bail!(
            "remote index too large: {} entries (max {})",
            remote_index.entries.len(),
            crate::protocol::MAX_INDEX_ENTRIES
        );
    }

    // Note: we deliberately do NOT compare the remote's folder_id against our
    // own. folder_id is a per-device local storage key, not a shared
    // capability token — each side stores the same logical folder under a
    // different row id, so cross-peer equality would wrongly reject
    // legitimate syncs. The peer has already been authenticated by its TLS
    // certificate fingerprint above, which is the authorization boundary for
    // this single-folder endpoint.

    // Build and send our index
    // When the initiating peer stores a remote_path for this pair, that is
    // where this folder's copy lives on us — relocate our serving root there
    // instead of the registered local_path (§17). Missing directory is created.
    let served_root_path: PathBuf = match &remote_index.remote_path {
        Some(rp) if !rp.trim().is_empty() => {
            let p = PathBuf::from(rp);
            tokio::fs::create_dir_all(&p).await?;
            p
        }
        _ => PathBuf::from(local_path),
    };
    let served_root_str = served_root_path.to_string_lossy().to_string();
    let local_entries = build_protocol_index(served_root_path.clone(), folder_id, device_id)?;
    let msg = SyncMessage::Index(Index {
        folder_id: folder_id.to_string(),
        device_id: device_id.to_string(),
        remote_path: None,
        entries: local_entries.clone(),
    });
    conn.write_all(&frame_message(&msg)?).await?;
    let root = Arc::new(SyncRoot::open(served_root_path)?);

    // Convert both indexes to domain Snapshots and run the pure reconciler
    let folder = FolderId(folder_id);
    let local_snap = SyncOrchestrator::index_to_snapshot(
        &Index {
            folder_id: folder_id.to_string(),
            device_id: device_id.to_string(),
            remote_path: None,
            entries: local_entries.clone(),
        },
        folder,
    );
    let remote_snap = SyncOrchestrator::index_to_snapshot(&remote_index, folder);
    let orch = SyncOrchestrator::new(root.clone(), state_store, folder);
    let plan = orch.reconcile_snapshots(&local_snap, &remote_snap);

    // Compute what they need from us so we can push it proactively
    let mut to_push_to_client: Vec<&IndexEntry> = plan
        .uploads
        .iter()
        .filter_map(|op| {
            if let SyncOperation::Upload { path, .. } = op {
                local_entries.iter().find(|e| e.path == path.0)
            } else {
                None
            }
        })
        .collect();
    // Conflicts: also push our local version (last-writer-wins policy).
    for op in &plan.conflicts {
        let path = op.path();
        if let Some(entry) = local_entries.iter().find(|e| e.path == path.0) {
            to_push_to_client.push(entry);
        }
    }

    // Live progress totals: what we push plus what the client pushes back.
    let client_to_push: Vec<&IndexEntry> = plan
        .downloads
        .iter()
        .filter_map(|op| {
            if let SyncOperation::Download { path, .. } = op {
                remote_index.entries.iter().find(|e| e.path == path.0)
            } else {
                None
            }
        })
        .collect();
    let total_files = (to_push_to_client.len() + client_to_push.len()) as u64;
    let total_bytes: u64 = to_push_to_client
        .iter()
        .chain(client_to_push.iter())
        .map(|e| e.size)
        .sum();
    let mut done_files = 0u64;
    let mut done_bytes = 0u64;
    let mut pushed_bytes = 0u64;
    let mut pulled_bytes = 0u64;
    emit_progress(
        &event_tx,
        folder_id,
        "starting",
        done_files,
        done_bytes,
        total_files,
        total_bytes,
    );

    // Push our files to client
    for entry in &to_push_to_client {
        let file_path = root.safe_join(&entry.path)?;
        let data = match tokio::fs::read(&file_path).await {
            Ok(d) => d,
            Err(_) => continue,
        };
        send_file_chunks_tls(conn, &entry.path, &data, folder_id).await?;
        pushed_bytes += data.len() as u64;
        done_files += 1;
        done_bytes += data.len() as u64;
        emit_progress(
            &event_tx,
            folder_id,
            "uploading",
            done_files,
            done_bytes,
            total_files,
            total_bytes,
        );
        record_history_row(
            &storage,
            folder_id,
            &entry.path,
            device_id,
            "push",
            entry.mtime,
            entry.size as i64,
            &entry.hash,
        );
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

    // Build expected hashes from the client's index for integrity verification.
    let expected_hashes: HashMap<String, Vec<u8>> = remote_index
        .entries
        .iter()
        .map(|e| (e.path.clone(), e.hash.clone()))
        .collect();

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

                    // Verify file integrity before committing to disk.
                    let actual_hash = blake3::hash(&data).as_bytes().to_vec();
                    if let Some(expected) = expected_hashes.get(&chunk.path) {
                        if &actual_hash != expected {
                            log::warn!(
                                "hash mismatch for {} — rejecting push from client",
                                chunk.path,
                            );
                            conn.write_all(&frame_message(&SyncMessage::Ack(Ack {
                                path: chunk.path.clone(),
                                success: false,
                                error: Some("hash mismatch".into()),
                            }))?)
                            .await?;
                            continue;
                        }
                    }

                    let target = root.safe_join(&chunk.path)?;
                    if let Some(parent) = target.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    let conflicted = backup_on_conflict(
                        &served_root_str,
                        &chunk.path,
                        &data,
                        &event_tx,
                        "remote",
                    )
                    .await?;
                    if conflicted {
                        conflicts += 1;
                    }
                    // Atomic write: temp file + rename to prevent corruption on crash.
                    let temp_path = target.with_extension(".ferrisync-tmp");
                    tokio::fs::write(&temp_path, &data).await?;
                    tokio::fs::rename(&temp_path, &target).await.map_err(|e| {
                        let _ = std::fs::remove_file(&temp_path);
                        anyhow::anyhow!("atomic rename failed: {e}")
                    })?;
                    received_pushes.push(chunk.path.clone());
                    pulled_bytes += data.len() as u64;
                    done_files += 1;
                    done_bytes += data.len() as u64;
                    emit_progress(
                        &event_tx,
                        folder_id,
                        "downloading",
                        done_files,
                        done_bytes,
                        total_files,
                        total_bytes,
                    );
                    record_history_row(
                        &storage,
                        folder_id,
                        &chunk.path,
                        device_id,
                        if conflicted { "conflict" } else { "pull" },
                        file_mtime_secs(&target),
                        data.len() as i64,
                        &actual_hash,
                    );

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
                    if path == "metadata.db" || path.len() > MAX_PATH_LEN {
                        continue;
                    }
                    let file_path = root.safe_join(path)?;
                    if let Ok(data) = tokio::fs::read(&file_path).await {
                        send_file_chunks_tls(conn, path, &data, folder_id).await?;
                        served_requests.push(path.clone());
                        pushed_bytes += data.len() as u64;
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
        "served session for {served_root_str}: sent {} ({sent_list}), answered requests for {} ({served_list}), received {} ({received_list}), conflicts {}",
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
        pushed_bytes,
        pulled_bytes,
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

/// Emit a live transfer-progress snapshot for a running session.
fn emit_progress(
    event_tx: &mpsc::Sender<crate::sync_engine::SyncEvent>,
    folder_id: i64,
    stage: &'static str,
    files_done: u64,
    files_total: u64,
    bytes_done: u64,
    bytes_total: u64,
) {
    let _ = event_tx.try_send(crate::sync_engine::SyncEvent::Progress {
        folder_id: folder_id.to_string(),
        stage: stage.to_string(),
        files_done,
        files_total,
        bytes_done,
        bytes_total,
    });
}

/// Best-effort persistence of a per-file history row. This feeds the UI
/// timeline and per-folder file states; failures are intentionally ignored
/// since history is informational only.
fn record_history_row(
    storage: &Storage,
    folder_id: i64,
    path: &str,
    device_id: &str,
    action: &str,
    mtime: i64,
    size: i64,
    hash: &[u8],
) {
    let _ = storage.record_history(crate::storage::HistoryRecord {
        folder_id,
        path,
        device_id,
        action,
        version: 0,
        mtime,
        hash,
        size,
    });
}

/// Unix seconds of a file's last-modified time, or 0 when unreadable.
fn file_mtime_secs(path: &std::path::Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

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

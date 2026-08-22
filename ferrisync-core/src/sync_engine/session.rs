use crate::crypto::CryptoProvider;
use crate::protocol::{
    frame_message, Ack, FileChunk, FileRequest, Index, IndexEntry, PairResponse, SyncMessage,
};
use crate::storage::Storage;
use crate::transport::tcp::TcpTransport;
use crate::transport::TransportConnector;
use crate::DeviceInfo;
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

const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks

/// Outcomes from a sync session.
#[derive(Debug, Default)]
pub struct SyncResult {
    pub pulled: Vec<String>,
    pub pushed: Vec<String>,
    pub conflicts: Vec<String>,
}

/// Run a complete bidirectional sync session as the initiating peer.
pub async fn run_sync_session(
    crypto: Arc<CryptoProvider>,
    _storage: Arc<Storage>,
    local_path: &str,
    remote_addr: std::net::SocketAddr,
    folder_id: i64,
    device_id: &str,
    event_tx: mpsc::Sender<crate::sync_engine::SyncEvent>,
) -> Result<SyncResult> {
    let transport = TcpTransport::new(crypto.clone());
    let mut conn = transport.connect(remote_addr).await?;

    // Build and send our index
    let local_index = build_index(PathBuf::from(local_path))?;
    let msg = SyncMessage::Index(Index {
        folder_id: folder_id.to_string(),
        entries: local_index.clone(),
    });
    conn.write_all(&frame_message(&msg)?).await?;

    // Receive remote index
    let remote_index = read_message(&mut conn).await?;
    let remote_index = match remote_index {
        SyncMessage::Index(idx) => idx,
        _ => anyhow::bail!("expected Index message"),
    };

    // Compute what we need
    let to_pull = compute_entries_to_pull(&local_index, &remote_index);
    let to_push = compute_entries_to_push(&local_index, &remote_index);

    let mut result = SyncResult::default();

    // Send files they need (push), then request+receive files we need (pull)
    // We push files first, then send file request, then receive responses & files
    for entry in &to_push {
        let file_path = PathBuf::from(local_path).join(&entry.path);
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

    // Send file request for files we need
    if !to_pull.is_empty() {
        let paths: Vec<String> = to_pull.iter().map(|e| e.path.clone()).collect();
        let req = SyncMessage::FileRequest(FileRequest {
            folder_id: folder_id.to_string(),
            paths,
        });
        conn.write_all(&frame_message(&req)?).await?;
    } else {
        // Send a sentinel to indicate we're done
        conn.write_all(&frame_message(&SyncMessage::Ack(Ack {
            path: String::new(),
            success: true,
            error: None,
        }))?)
        .await?;
    }

    // Handle incoming messages: FileChunks (files from server), Acks for
    // our pushes, and FileRequests. The session stays open until every
    // pushed file has been acknowledged and every requested file has
    // arrived — closing earlier with unread inbound data makes the kernel
    // send a TCP RST that destroys frames still queued on the peer.
    let mut incoming_files: HashMap<String, Vec<u8>> = HashMap::new();
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
        let msg = read_message(&mut conn).await;
        let msg = match msg {
            Ok(m) => m,
            Err(_) => break,
        };

        match msg {
            SyncMessage::FileChunk(chunk) => {
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

                if end >= total {
                    let data = incoming_files.remove(&chunk.path).unwrap();
                    let target = PathBuf::from(local_path).join(&chunk.path);
                    if let Some(parent) = target.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    if backup_on_conflict(local_path, &chunk.path, &data, &event_tx, device_id)
                        .await?
                    {
                        result.conflicts.push(chunk.path.clone());
                    }
                    tokio::fs::write(&target, &data).await?;

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
                for path in &req.paths {
                    let file_path = PathBuf::from(local_path).join(path);
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

    Ok(result)
}

/// Listen for incoming connections as a server.
/// Accepts both pairing requests and sync sessions on the same port.
///
/// The accept loop runs in a spawned task; the returned handle can be used to
/// await (or abort) it. The loop exits when `shutdown` fires or the sender is
/// dropped.
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
) -> Result<tokio::task::JoinHandle<()>> {
    let listener = TcpListener::bind(addr).await?;

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

                    tokio::spawn(async move {
                        let config = match crypto.server_config().await {
                            Ok(c) => c,
                            Err(e) => {
                                log::error!("TLS config failed: {e}");
                                return;
                            }
                        };
                        let acceptor = tokio_rustls::TlsAcceptor::from(config);
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

                        match read_tls_message(&mut tls).await {
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
                                    if let Err(e) =
                                        storage.upsert_device(&req.device_id, &req.device_name, None)
                                    {
                                        log::error!("failed to store paired device: {e}");
                                    } else {
                                        gate.paired(&req.device_name, &req.device_id).await;
                                    }
                                }
                            }
                            Ok(SyncMessage::Index(idx)) => {
                                if let Err(e) = handle_server_session(
                                    &mut tls,
                                    crypto,
                                    storage,
                                    &local_path,
                                    folder_id,
                                    event_tx,
                                    idx,
                                )
                                .await
                                {
                                    log::error!("session error: {e}");
                                }
                            }
                            Ok(_) => {
                                log::warn!("unexpected message type from incoming connection");
                            }
                            Err(e) => {
                                log::error!("failed to read initial message: {e}");
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

    Ok(handle)
}

/// Read first message from a TLS stream and dispatch to `handle_server_session`.
/// Convenience wrapper used by tests.
pub async fn handle_server_session_with_read(
    conn: &mut tokio_rustls::TlsStream<tokio::net::TcpStream>,
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    local_path: &str,
    folder_id: i64,
    event_tx: mpsc::Sender<crate::sync_engine::SyncEvent>,
) -> Result<()> {
    let remote_msg = read_tls_message(conn).await?;
    let remote_index = match remote_msg {
        SyncMessage::Index(idx) => idx,
        _ => anyhow::bail!("expected Index"),
    };
    handle_server_session(
        conn,
        crypto,
        storage,
        local_path,
        folder_id,
        event_tx,
        remote_index,
    )
    .await
}

/// Handle a sync session from the server side.
pub async fn handle_server_session(
    conn: &mut tokio_rustls::TlsStream<tokio::net::TcpStream>,
    _crypto: Arc<CryptoProvider>,
    _storage: Arc<Storage>,
    local_path: &str,
    folder_id: i64,
    event_tx: mpsc::Sender<crate::sync_engine::SyncEvent>,
    remote_index: Index,
) -> Result<()> {
    // Build and send our index
    let local_entries = build_index(PathBuf::from(local_path))?;
    let msg = SyncMessage::Index(Index {
        folder_id: folder_id.to_string(),
        entries: local_entries.clone(),
    });
    conn.write_all(&frame_message(&msg)?).await?;

    // Compute what they need from us so we can push it proactively
    let to_push_to_client = compute_entries_to_push(&local_entries, &remote_index);

    // Push our files to client
    for entry in &to_push_to_client {
        let file_path = PathBuf::from(local_path).join(&entry.path);
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
    let mut got_eof = false;

    loop {
        let msg = read_tls_message(conn).await;
        let msg = match msg {
            Ok(m) => m,
            Err(_) => break,
        };

        match msg {
            SyncMessage::FileChunk(chunk) => {
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

                if end >= total {
                    let data = incoming_files.remove(&chunk.path).unwrap();
                    let target = PathBuf::from(local_path).join(&chunk.path);
                    if let Some(parent) = target.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    backup_on_conflict(local_path, &chunk.path, &data, &event_tx, "remote").await?;
                    tokio::fs::write(&target, &data).await?;

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
                // Client requests files from us
                for path in &req.paths {
                    let file_path = PathBuf::from(local_path).join(path);
                    if let Ok(data) = tokio::fs::read(&file_path).await {
                        send_file_chunks_tls(conn, path, &data, folder_id).await?;
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
    let target = PathBuf::from(local_path).join(path);
    if !target.exists() {
        return Ok(false);
    }
    let existing = tokio::fs::read(&target).await?;
    if existing == incoming_data {
        return Ok(false);
    }
    // Content differs — rename to .bak before overwriting
    let bak = PathBuf::from(format!("{}.bak", target.display()));
    tokio::fs::rename(&target, &bak).await?;

    let loser_label = if winner_label == "remote" {
        "local"
    } else {
        "remote"
    };
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
    let mut payload = vec![0u8; len];
    conn.read_exact(&mut payload).await?;
    Ok(bincode::deserialize(&payload)?)
}

// ── Index helpers ──

/// Build a file index by scanning a directory.
fn build_index(root: PathBuf) -> Result<Vec<IndexEntry>> {
    let mut entries = Vec::new();
    if !root.exists() {
        return Ok(entries);
    }
    scan_dir(&root, &root, &mut entries)?;
    Ok(entries)
}

fn scan_dir(root: &PathBuf, dir: &PathBuf, entries: &mut Vec<IndexEntry>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            scan_dir(root, &path, entries)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        // Skip internal database files
        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        if fname == "metadata.db" {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let meta = std::fs::metadata(&path)?;
        let mtime = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;
        let data = std::fs::read(&path)?;
        let hash = blake3::hash(&data).as_bytes().to_vec();
        entries.push(IndexEntry {
            path: relative,
            local_version: mtime as u64,
            remote_version: 0,
            mtime,
            size: meta.len(),
            hash,
        });
    }
    Ok(())
}

/// Entries we need to pull (remote has it, we don't, or remote's is newer).
fn compute_entries_to_pull(local: &[IndexEntry], remote: &Index) -> Vec<IndexEntry> {
    let local_map: HashMap<&str, &IndexEntry> =
        local.iter().map(|e| (e.path.as_str(), e)).collect();
    remote
        .entries
        .iter()
        .filter(|r| {
            local_map
                .get(r.path.as_str())
                .is_none_or(|l| l.hash != r.hash && r.mtime > l.mtime)
        })
        .cloned()
        .collect()
}

/// Entries we need to push (we have it, remote doesn't, or ours is newer).
fn compute_entries_to_push(local: &[IndexEntry], remote: &Index) -> Vec<IndexEntry> {
    let remote_map: HashMap<&str, &IndexEntry> = remote
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e))
        .collect();
    local
        .iter()
        .filter(|l| {
            remote_map
                .get(l.path.as_str())
                .is_none_or(|r| l.hash != r.hash && l.mtime > r.mtime)
        })
        .cloned()
        .collect()
}

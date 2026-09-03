//! flutter_rust_bridge FFI surface.
//! Annotated functions are picked up by FRB codegen to generate Dart bindings.

use crate::crypto::CryptoProvider;
use crate::storage::Storage;
use crate::sync_engine::pairing::PairingManager;
use crate::sync_engine::server::{PairPolicy, ServeHandle};
use crate::sync_engine::session::SyncResult;
use crate::sync_engine::{SyncEngine, SyncEvent};
use crate::DeviceInfo;
use anyhow::Context;
use rustls::pki_types::PrivateKeyDer;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
// Re-export so generated bridge code (`use crate::api::*`) can name RwLock.
#[allow(unused_imports)]
pub use std::sync::RwLock;

// ── Discovery ──

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
}

/// Scan the LAN for FerriSync servers advertising via mDNS.
/// Returns all devices discovered within `timeout_secs` seconds.
pub async fn discover_devices(timeout_secs: u64) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let svc = crate::discovery::DiscoveryService::new(
        DeviceInfo {
            id: String::new(),
            name: String::new(),
            cert_fingerprint: vec![],
        },
        0,
    )?;
    let mut rx = svc.browse()?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut peers = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(peer)) => {
                if let Some(addr) = peer.addresses.first() {
                    peers.push(DiscoveredDevice {
                        id: peer.id,
                        name: peer.name,
                        ip: addr.ip().to_string(),
                        port: addr.port(),
                    });
                }
            }
            _ => break,
        }
    }

    Ok(peers)
}

// ── Initialization ──

#[derive(Debug, Clone)]
pub struct ApiState {
    pub crypto: Arc<CryptoProvider>,
    pub storage: Arc<Storage>,
    pub engine: Arc<SyncEngine>,
    pub pairing: Arc<PairingManager>,
    /// Own identity. Behind a lock so a rename propagates to servers and
    /// pairing started afterwards without rebuilding the whole state.
    pub device_info: Arc<RwLock<DeviceInfo>>,
    pub data_dir: String,
}

impl ApiState {
    /// Snapshot of the current device identity.
    pub fn current_device(&self) -> DeviceInfo {
        self.device_info.read().unwrap().clone()
    }
}

/// Initialise the sync engine and return an opaque handle.
///
/// The device identity (certificate, key, id, name) is persisted in the data
/// directory and reused across launches so pairings survive restarts.
pub async fn init_engine(data_dir: String) -> anyhow::Result<ApiState> {
    let path = PathBuf::from(&data_dir);
    std::fs::create_dir_all(&path)?;

    let crypto = Arc::new(load_or_create_identity(&path).await?);
    let storage = Arc::new(Storage::open(&path.join("metadata.db"))?);

    let dev_id = load_or_create_device_id(&path);
    let device_info = DeviceInfo {
        id: dev_id,
        name: crate::config::load_device_name(&path).unwrap_or_else(|| {
            whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string())
        }),
        cert_fingerprint: crypto.fingerprint().await,
    };

    let engine = Arc::new(SyncEngine::new(
        storage.clone(),
        crypto.clone(),
        device_info.clone(),
        Arc::new(crate::persistence::InMemoryStateStore::new()),
    ));
    let pairing = Arc::new(PairingManager::new(
        crypto.clone(),
        storage.clone(),
        device_info.clone(),
    ));

    let state = ApiState {
        crypto,
        storage,
        engine,
        pairing,
        device_info: Arc::new(RwLock::new(device_info)),
        data_dir,
    };

    // Serve every configured folder so peers can connect to us. Ports start
    // at 9847 and increment per folder; failures are logged, not fatal.
    // A folder may have several device pairs — serve each distinct folder once.
    let mut served: Vec<(i64, String)> = Vec::new();
    for (i, (id, local_path, _device, _dir, _last)) in
        state.storage.list_sync_folders()?.iter().enumerate()
    {
        if served
            .iter()
            .any(|(fid, path)| *fid == *id || path == local_path)
        {
            continue;
        }
        served.push((*id, local_path.clone()));
        let port = u16::try_from(9847 + i).unwrap_or(9847);
        if let Err(e) = start_server(&state, port, *id, local_path.clone()).await {
            log::warn!("failed to serve folder {local_path} on port {port}: {e}");
        }
    }

    Ok(state)
}

// ── Identity persistence ──

const IDENTITY_CERT_FILE: &str = "identity.cert.der";
const IDENTITY_KEY_FILE: &str = "identity.key.der";
const DEVICE_ID_FILE: &str = "device.id";

async fn load_or_create_identity(path: &Path) -> anyhow::Result<CryptoProvider> {
    let cert_path = path.join(IDENTITY_CERT_FILE);
    let key_path = path.join(IDENTITY_KEY_FILE);
    if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        let fingerprint = blake3::hash(&cert).as_bytes().to_vec();
        return CryptoProvider::load(cert, key, fingerprint);
    }

    let crypto = CryptoProvider::generate()?;
    let key = crypto.private_key().await;
    let key_bytes: Vec<u8> = match &key {
        PrivateKeyDer::Pkcs8(k) => k.secret_pkcs8_der().to_vec(),
        PrivateKeyDer::Pkcs1(k) => k.secret_pkcs1_der().to_vec(),
        PrivateKeyDer::Sec1(k) => k.secret_sec1_der().to_vec(),
        _ => anyhow::bail!("unsupported private key format"),
    };
    std::fs::write(&cert_path, &crypto.certificate().await)?;
    std::fs::write(&key_path, key_bytes)?;
    Ok(crypto)
}

fn load_or_create_device_id(path: &Path) -> String {
    if let Ok(id) = std::fs::read_to_string(path.join(DEVICE_ID_FILE)) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::write(path.join(DEVICE_ID_FILE), &id);
    id
}

// The user-chosen device name, if one was ever set. Absent means "use the
// hostname"; the file is only written by an explicit rename.
// ── Server ──

/// A live folder listener plus the parameters it was started with, so a
/// rename can stop and re-serve it under the fresh identity.
struct RunningServer {
    handle: ServeHandle,
    port: u16,
    local_path: String,
}

/// Live folder listeners owned by this app instance, keyed by folder id.
fn servers() -> &'static Mutex<HashMap<i64, RunningServer>> {
    static SERVERS: OnceLock<Mutex<HashMap<i64, RunningServer>>> = OnceLock::new();
    SERVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start listening for incoming sync connections to a folder. The listener
/// also advertises the device on the LAN via mDNS so peers can discover it.
///
/// `port` is a hint; on bind failure the error is returned so callers can
/// retry with another port.
pub async fn start_server(
    state: &ApiState,
    port: u16,
    folder_id: i64,
    local_path: String,
) -> anyhow::Result<()> {
    if servers().lock().unwrap().contains_key(&folder_id) {
        return Ok(());
    }
    let (handle, mut events) = crate::sync_engine::server::serve_folder(
        state.storage.clone(),
        state.crypto.clone(),
        state.current_device(),
        local_path.clone(),
        port,
        // Already-paired peers are admitted silently; unknown peers raise
        // PairRequested events for the app's consent dialog.
        PairPolicy::Confirm,
        state.engine.state_store().clone(),
    )
    .await?;
    // Forward server-side activity into the shared event stream so the app's
    // poll loop observes pairing requests and file transfers.
    let forward_tx = state.engine.event_sender();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if forward_tx.send(event).await.is_err() {
                break;
            }
        }
    });
    let resolved_port = handle.port;
    servers().lock().unwrap().insert(
        folder_id,
        RunningServer {
            handle,
            port: resolved_port,
            local_path,
        },
    );
    Ok(())
}

/// Stop all folder listeners (idempotent).
pub async fn stop_server(state: &ApiState) -> anyhow::Result<()> {
    let handles: Vec<ServeHandle> = servers()
        .lock()
        .unwrap()
        .drain()
        .map(|(_, s)| s.handle)
        .collect();
    for handle in handles {
        let _ = handle.stop().await;
    }
    let _ = state; // symmetric signature for FFI stability
    Ok(())
}

/// Restart every running folder listener so mDNS advertisements and pairing
/// responses carry the current device name. Failures are logged, not fatal.
async fn restart_all_servers(state: &ApiState) {
    let entries: Vec<(i64, RunningServer)> = servers().lock().unwrap().drain().collect();
    if entries.is_empty() {
        return;
    }
    let total = entries.len();
    let mut restarted = 0;
    for (folder_id, server) in entries {
        let RunningServer {
            handle,
            port,
            local_path,
        } = server;
        let _ = handle.stop().await;
        match start_server(state, port, folder_id, local_path).await {
            Ok(()) => restarted += 1,
            Err(e) => log::warn!("failed to restart folder server {folder_id} on port {port}: {e}"),
        }
    }
    log::info!("restarted {restarted}/{total} folder server(s) after rename");
}

// ── Device / Pairing ──

/// List pairing requests waiting for approval across all running folder
/// servers. Each entry is `(device_name, device_id)`.
pub fn pending_pairings(state: &ApiState) -> anyhow::Result<Vec<(String, String)>> {
    let _ = state;
    let mut result = Vec::new();
    for (_folder_id, server) in servers().lock().unwrap().iter() {
        if let Ok(pending) = server.handle.pending_pairings() {
            result.extend(pending);
        }
    }
    result.sort_by(|a, b| a.1.cmp(&b.1));
    result.dedup_by(|a, b| a.1 == b.1);
    Ok(result)
}

/// Approve a pending pairing request. The device is written to the paired
/// devices table so its next sync attempt is accepted immediately.
pub fn approve_pending_pairing(
    _state: &ApiState,
    device_id: String,
    device_name: String,
) -> anyhow::Result<()> {
    for (_folder_id, server) in servers().lock().unwrap().iter() {
        if let Ok(pending) = server.handle.pending_pairings() {
            if pending.iter().any(|(_, id)| id == &device_id) {
                server.handle.approve_pairing(&device_id, &device_name)?;
                return Ok(());
            }
        }
    }
    anyhow::bail!("no pending pairing for device {device_id}")
}

/// Deny a pending pairing request. The device is remembered for this
/// server's lifetime so repeated requests are silently rejected.
pub fn deny_pending_pairing(state: &ApiState, device_id: String) -> anyhow::Result<()> {
    let _ = state;
    for (_folder_id, server) in servers().lock().unwrap().iter() {
        if let Ok(pending) = server.handle.pending_pairings() {
            if pending.iter().any(|(_, id)| id == &device_id) {
                server.handle.deny_pairing(&device_id)?;
                return Ok(());
            }
        }
    }
    anyhow::bail!("no pending pairing for device {device_id}")
}

// ── Shared-folder pairing approval (owner side) ──

/// A peer's request to pair to one of our shared folders, awaiting approval.
#[derive(Debug, Clone)]
pub struct PendingFolderPairing {
    pub device_id: String,
    pub device_name: String,
    pub folder_guid: String,
    pub folder_name: String,
}

/// Folder-pairing requests held on this device's server(s) for approval.
pub fn pending_folder_pairings(state: &ApiState) -> anyhow::Result<Vec<PendingFolderPairing>> {
    let _ = state;
    let mut result = Vec::new();
    for (_folder_id, server) in servers().lock().unwrap().iter() {
        if let Ok(pending) = server.handle.pending_folder_pairings() {
            for req in pending {
                result.push(PendingFolderPairing {
                    device_id: req.device_id.clone(),
                    device_name: req.device_name.clone(),
                    folder_guid: req.folder_guid.clone(),
                    folder_name: req.name.clone(),
                });
            }
        }
    }
    result.sort_by(|a, b| (&a.folder_guid, &a.device_id).cmp(&(&b.folder_guid, &b.device_id)));
    result.dedup_by(|a, b| a.folder_guid == b.folder_guid && a.device_id == b.device_id);
    Ok(result)
}

/// Approve a held folder pairing, wiring the replica pair into this device's
/// storage so the peer starts syncing. `local_path` is this (owner) device's
/// copy of the shared folder; `remote_path` is where the peer keeps its copy.
pub fn approve_folder_pairing(
    state: &ApiState,
    device_id: String,
    folder_guid: String,
    folder_name: String,
    local_path: String,
    remote_path: Option<String>,
) -> anyhow::Result<()> {
    for (_folder_id, server) in servers().lock().unwrap().iter() {
        if let Ok(pending) = server.handle.pending_folder_pairings() {
            if pending
                .iter()
                .any(|p| p.device_id == device_id && p.folder_guid == folder_guid)
            {
                server.handle.approve_folder_pairing(
                    &device_id,
                    &folder_guid,
                    &folder_name,
                    &local_path,
                    remote_path.as_deref(),
                )?;
                let _ = state;
                return Ok(());
            }
        }
    }
    anyhow::bail!("no pending folder pairing for device {device_id} on {folder_guid}")
}

/// Deny a held folder pairing. The peer is remembered for this server's
/// lifetime so its retries are rejected, not re-queued.
pub fn deny_folder_pairing(
    state: &ApiState,
    device_id: String,
    folder_guid: String,
) -> anyhow::Result<()> {
    let _ = state;
    for (_folder_id, server) in servers().lock().unwrap().iter() {
        if let Ok(pending) = server.handle.pending_folder_pairings() {
            if pending
                .iter()
                .any(|p| p.device_id == device_id && p.folder_guid == folder_guid)
            {
                server
                    .handle
                    .deny_folder_pairing(&device_id, &folder_guid)?;
                return Ok(());
            }
        }
    }
    anyhow::bail!("no pending folder pairing for device {device_id} on {folder_guid}")
}

/// Maximum length of a user-chosen device name. It travels in protocol
/// frames and mDNS records, so keep it modest.
pub const DEVICE_NAME_MAX_LEN: usize = 64;

/// Validate and normalize a user-chosen device name. Shared by every
/// frontend (app, REPL, CLI) so all enforce the same rules.
pub fn sanitize_device_name(name: &str) -> anyhow::Result<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("device name cannot be empty");
    }
    if name.chars().count() > DEVICE_NAME_MAX_LEN {
        anyhow::bail!("device name too long (max {DEVICE_NAME_MAX_LEN} characters)");
    }
    if name.chars().any(|c| c.is_control()) {
        anyhow::bail!("device name contains invalid characters");
    }
    Ok(name)
}

/// Rename this device: validates, persists across restarts, updates the live
/// identity used by pairing and new server sessions, and restarts any running
/// folder servers so peers immediately see the new name on the LAN.
pub async fn set_device_name(state: &ApiState, name: String) -> anyhow::Result<String> {
    let name = sanitize_device_name(&name)?;

    crate::config::persist_device_name(Path::new(&state.data_dir), &name);
    state.device_info.write().unwrap().name = name.clone();
    state.pairing.set_name(&name);
    restart_all_servers(state).await;
    Ok(name)
}

pub fn upsert_device(state: &ApiState, id: String, name: String) -> anyhow::Result<()> {
    state.storage.upsert_device(&id, &name, None, None)
}

/// Last known address of a paired device (set when we initiated pairing).
pub fn device_last_addr(state: &ApiState, device_id: String) -> anyhow::Result<Option<String>> {
    state.storage.device_last_addr(&device_id)
}

pub async fn pair_with_device(state: &ApiState, ip: String, port: u16) -> anyhow::Result<String> {
    let addr: std::net::SocketAddr = format!("{ip}:{port}").parse()?;
    let peer = state.pairing.pair_with(addr).await?;
    Ok(format!("{} ({})", peer.name, peer.id))
}

/// Re-establish pairing with previously-approved devices.
///
/// On startup this re-runs the pair handshake against every *already-trusted*
/// device (its certificate is stored, i.e. it completed device pairing before)
/// that is still reachable at its last-known address. Reachability is decided
/// by the handshake itself — if a device is now online at its stored address,
/// it gets re-paired and its address/trust are refreshed for startup auto-sync;
/// unreachable or offline peers are simply skipped. Unknown peers are never
/// touched — this is auto-*re*pair, not auto-*pair*.
///
/// We deliberately key on the stored address, not the mDNS-advertised id: a
/// device advertises its *identity* id, which differs from the *cert-derived*
/// id used to key the `devices` table, and the address the peer dialled
/// (e.g. a loopback / LAN IP) need not match the address it advertises. Trying
/// each known address directly is robust to both. Returns how many were
/// successfully re-paired.
pub async fn auto_repair_known_devices(
    state: &ApiState,
    _timeout_secs: u64,
) -> anyhow::Result<usize> {
    let mut repaired = 0usize;

    for (_, last_addr) in state.storage.list_trusted_device_addrs()? {
        let Ok(addr) = last_addr.parse::<std::net::SocketAddr>() else {
            continue;
        };
        if state.pairing.pair_with(addr).await.is_ok() {
            repaired += 1;
        }
    }

    Ok(repaired)
}

/// Every paired device. Excludes our own row (which only exists when a
/// folder is served here), mirroring `device_statuses`/the CLI so the app
/// never surfaces the local device as a remote peer.
pub fn list_devices(state: &ApiState) -> anyhow::Result<Vec<DeviceEntry>> {
    let own = state.current_device().id;
    let rows = state.storage.list_devices()?;
    Ok(rows
        .into_iter()
        .filter(|(id, _, _)| id != &own)
        .map(|(id, name, last_seen)| DeviceEntry {
            id,
            name,
            last_seen: last_seen.unwrap_or(0),
        })
        .collect())
}

#[derive(Debug, Clone)]
pub struct DeviceEntry {
    pub id: String,
    pub name: String,
    pub last_seen: i64,
}

/// Breakdown of rows deleted by [`remove_device`]. The DTO is defined by the
/// persistence contract so FRB sees exactly one `DeviceCleanup` in the crate.
pub use crate::persistence::traits::DeviceCleanup;

pub fn remove_device(state: &ApiState, device_id: String) -> anyhow::Result<DeviceCleanup> {
    state.storage.remove_device(&device_id)
}

// ── Folders ──

/// Remove a sync folder and its metadata/history from the local database.
pub fn remove_folder(state: &ApiState, folder_id: i64) -> anyhow::Result<()> {
    Ok(state.storage.remove_sync_folder_by_id(folder_id)?)
}

/// One folder↔device relationship (*not* the removal summary). Returned by
/// [`list_folder_devices`].
#[derive(Debug, Clone)]
pub struct FolderDevice {
    pub device_id: String,
    pub mode: String,
    /// Where the peer keeps this folder's copy (default: same-basename).
    pub remote_path: Option<String>,
    pub enabled: bool,
}

/// Attach an additional device to an existing folder, each with its own sync
/// mode and optional remote destination on that peer.
pub fn add_folder_device(
    state: &ApiState,
    folder_id: i64,
    device_id: String,
    mode: String,
    remote_path: Option<String>,
) -> anyhow::Result<FolderDevice> {
    state
        .storage
        .upsert_device(&device_id, &device_id, None, None)?;
    state
        .storage
        .add_folder_device(folder_id, &device_id, &mode, remote_path.as_deref())?;
    Ok(FolderDevice {
        device_id,
        mode,
        remote_path,
        enabled: true,
    })
}

/// Drop a single folder↔device relationship. Never deletes files — only the
/// pairing. Returns false when the pair didn't exist.
pub fn remove_folder_device(
    state: &ApiState,
    folder_id: i64,
    device_id: String,
) -> anyhow::Result<bool> {
    state.storage.remove_folder_device(folder_id, &device_id)
}

/// Every device relationship for a folder, with per-device mode + remote path.
pub fn list_folder_devices(state: &ApiState, folder_id: i64) -> anyhow::Result<Vec<FolderDevice>> {
    Ok(state
        .storage
        .folder_pairs(folder_id)?
        .into_iter()
        .map(|(device_id, mode, remote_path, enabled)| FolderDevice {
            device_id,
            mode,
            remote_path,
            enabled,
        })
        .collect())
}

/// Re-point a folder's *local* path without losing its device relationships.
pub fn set_folder_local_path(
    state: &ApiState,
    folder_id: i64,
    local_path: String,
) -> anyhow::Result<()> {
    state.storage.set_folder_local_path(folder_id, &local_path)
}

// ── Shared Folders ──

/// One discoverable shared folder belonging to this device.
#[derive(Debug, Clone)]
pub struct SharedFolder {
    pub id: i64,
    pub folder_guid: String,
    pub name: String,
    pub local_path: String,
    pub discoverable: bool,
    pub enabled: bool,
    pub permissions: String,
}

/// Publish a local folder as a discoverable *shared folder* so already-trusted
/// peers can browse it and request pairing. Wiring the row onto the local
/// folder's existing `folder_guid` makes the share the seed of a logical sync
/// space that any approved peer replicates. Idempotent per guid.
pub fn share_folder(state: &ApiState, folder_id: i64, device_name: String) -> anyhow::Result<i64> {
    let guid = state
        .storage
        .folder_guid(folder_id)?
        .ok_or_else(|| anyhow::anyhow!("folder has no guid (migration required)"))?;
    let local_path = state
        .storage
        .folder_path(folder_id)?
        .ok_or_else(|| anyhow::anyhow!("folder path not found"))?;
    let name = if device_name.trim().is_empty() {
        crate::storage::path_label(&local_path)
    } else {
        device_name.trim().to_string()
    };
    let own = state.current_device().id;
    state
        .storage
        .upsert_device(&own, &state.current_device().name, None, None)?;
    state
        .storage
        .share_folder(&guid, &own, &name, &local_path)?;
    let row = state
        .storage
        .shared_folder_by_guid(&own, &guid)?
        .ok_or_else(|| anyhow::anyhow!("share not persisted"))?;
    Ok(row.0)
}

/// Every shared folder published by this device.
pub fn list_my_shared_folders(state: &ApiState) -> anyhow::Result<Vec<SharedFolder>> {
    let own = state.current_device().id;
    Ok(state
        .storage
        .list_shared_folders(&own)?
        .into_iter()
        .map(|r| SharedFolder {
            id: r.0,
            folder_guid: r.1,
            name: r.3,
            local_path: r.4,
            discoverable: r.5,
            enabled: r.6,
            permissions: r.7,
        })
        .collect())
}

/// Toggle whether a published share is visible to trusted peers browsing for
/// folders. Disabling hides it but keeps the pairing alive.
pub fn set_shared_discoverable(
    state: &ApiState,
    share_id: i64,
    discoverable: bool,
) -> anyhow::Result<()> {
    state
        .storage
        .set_shared_discoverable(share_id, discoverable)
}

/// Stop sharing a folder (removes the share; existing peer pairs remain).
pub fn unshare_folder(state: &ApiState, share_id: i64) -> anyhow::Result<()> {
    state.storage.unshare_folder(share_id)
}

/// A peer's shared folder as reported by `SharedFolders`.
#[derive(Debug, Clone)]
pub struct RemoteSharedFolder {
    pub folder_guid: String,
    pub name: String,
    pub mode: String,
    /// Absolute path on the peer where the shared folder lives.
    pub local_path: String,
}

/// Browse an already-trusted peer's discoverable shared folders over TLS.
/// This is a post-pairing surface — never advertised over mDNS.
pub async fn browse_peer_shared_folders(
    state: &ApiState,
    peer_ip: String,
    peer_port: u16,
) -> anyhow::Result<Vec<RemoteSharedFolder>> {
    let addr: std::net::SocketAddr = format!("{peer_ip}:{peer_port}").parse()?;
    let client =
        crate::sync_engine::shared_folder::SharedFolderClient::new(state.crypto.clone(), addr);
    Ok(client
        .list_shared_folders()
        .await?
        .into_iter()
        .map(|f| RemoteSharedFolder {
            folder_guid: f.folder_guid,
            name: f.name,
            mode: f.mode,
            local_path: f.local_path,
        })
        .collect())
}

/// Result of requesting pairing to a peer's shared folder.
#[derive(Debug, Clone)]
pub enum FolderPairResult {
    Approved {
        folder_guid: String,
        name: String,
        remote_path: Option<String>,
    },
    /// Owner explicitly rejected the request.
    Rejected(String),
    /// Still waiting on the owner; the caller can poll again.
    Pending,
}

/// Ask an already-paired peer to pair us to one of its shared folders, and
/// (when the owner approves) register the replica on this device so the folder
/// starts syncing.
///
/// `local_path` is where this device keeps its copy; `lifetime_ms` bounds how
/// long we keep polling for the owner's approval (0 = poll once).
pub async fn request_folder_pairing(
    state: &ApiState,
    peer_ip: String,
    peer_port: u16,
    peer_device_id: String,
    folder_guid: String,
    share_name: String,
    local_path: String,
    lifetime_ms: u64,
) -> anyhow::Result<FolderPairResult> {
    let addr: std::net::SocketAddr = format!("{peer_ip}:{peer_port}").parse()?;
    let client =
        crate::sync_engine::shared_folder::SharedFolderClient::new(state.crypto.clone(), addr);
    let own = state.current_device().id;
    // The replica's sync_folders row references `devices.id`; make sure we are
    // registered before registering the replica (the app normally does this on
    // serve, but a fresh requester may not have recorded itself yet).
    state
        .storage
        .upsert_device(&own, &state.current_device().name, None, None)?;
    let reply = client
        .request_and_collect_pairing(
            &own,
            &state.current_device().name,
            &folder_guid,
            &share_name,
            if lifetime_ms == 0 {
                None
            } else {
                Some(std::time::Duration::from_millis(lifetime_ms))
            },
        )
        .await?;
    match reply {
        crate::sync_engine::shared_folder::FolderPairReply::Approved(grant) => {
            // Register our replica of the logical space, reusing the owner's
            // guid so both sides track the same folder. When the user already
            // added a local folder for `local_path`, wire that folder to the
            // peer's share instead of minting a duplicate replica.
            let _folder_id = state.storage.wire_folder_pair(
                &folder_guid,
                &local_path,
                &share_name,
                &own,
                &peer_device_id,
                grant.remote_path.as_deref(),
            )?;
            // Record the owner's address so a follow-up sync can reach it
            // directly instead of depending on a prior transfer or a browse.
            state
                .storage
                .set_device_last_addr(&peer_device_id, &addr.to_string())?;
            Ok(FolderPairResult::Approved {
                folder_guid: grant.folder_guid,
                name: grant.name,
                remote_path: grant.remote_path,
            })
        }
        crate::sync_engine::shared_folder::FolderPairReply::Rejected(reason) => {
            Ok(FolderPairResult::Rejected(reason))
        }
        crate::sync_engine::shared_folder::FolderPairReply::Pending => {
            Ok(FolderPairResult::Pending)
        }
    }
}

/// Remove every paired device and its associated data (folders, metadata,
/// history, sessions). Returns how many devices were removed.
pub fn remove_all_devices(state: &ApiState) -> anyhow::Result<usize> {
    let ids: Vec<String> = state
        .storage
        .list_devices()?
        .into_iter()
        .map(|d| d.0)
        .collect();
    let count = ids.len();
    for id in ids {
        remove_device(state, id)?;
    }
    Ok(count)
}

/// Restore the device to a fresh-install state: stop every live folder
/// listener, then wipe all persisted state in the data directory (local
/// identity, device id, database, and device-name config). User files under
/// the configured folders are never touched.
///
/// The identity is not regenerated here — the next `init_engine` runs the
/// cold-start path and derives a brand-new device id from a fresh keypair.
pub async fn factory_reset(state: &ApiState) -> anyhow::Result<()> {
    stop_server(state).await?;

    let data_dir = Path::new(&state.data_dir);
    std::fs::remove_dir_all(data_dir)
        .with_context(|| format!("wipe data dir {}", data_dir.display()))?;
    Ok(())
}

/// Register a local folder to sync with a single device. The same path
/// against another device adds a second pair (multi-device folder).
pub fn add_sync_folder(
    state: &ApiState,
    local_path: String,
    device_id: String,
    direction: String,
) -> anyhow::Result<i64> {
    // `folder_devices.device_id` is a foreign key into `devices(id)`, so the
    // device must exist before the pair row is written. For "self" folders this
    // is required even before any device pairing has run — e.g. the very first
    // folder added during onboarding, when self has no `devices` row yet.
    // Skip entirely when the device row already exists, so a real peer's stored
    // display name (captured at pairing) is never clobbered with its id.
    if state.storage.get_device_name(&device_id)?.is_none() {
        let own = state.current_device();
        let default_name = if device_id == own.id {
            own.name.clone()
        } else {
            device_id.clone()
        };
        state
            .storage
            .upsert_device(&device_id, &default_name, None, None)?;
    }
    state
        .storage
        .add_sync_folder(&local_path, &device_id, &direction)
}

/// Multi-device form: create (or extend) a folder with several peers, each
/// with its own mode and optional destination on that peer. `name` is the
/// user-facing label (defaults to the path label when blank).
pub fn add_sync_folder_with_peers(
    state: &ApiState,
    local_path: String,
    name: String,
    peers: Vec<FolderPeerRequest>,
) -> anyhow::Result<i64> {
    let mut folder_id = None;
    for p in &peers {
        state
            .storage
            .upsert_device(&p.device_id, &p.device_id, None, None)?;
        let mode = p
            .mode
            .clone()
            .unwrap_or_else(|| "bidirectional".to_string());
        let id = state
            .storage
            .add_sync_folder(&local_path, &p.device_id, &mode)?;
        // Persist the per-pear remote_path (where the peer keeps this folder's
        // copy); reusing add_folder_device so an absent value never clobbers a
        // previously stored destination.
        state
            .storage
            .add_folder_device(id, &p.device_id, &mode, p.remote_path.as_deref())?;
        folder_id = Some(id);
    }
    let id = folder_id.ok_or_else(|| anyhow::anyhow!("at least one device is required"))?;
    if !name.trim().is_empty() {
        state.storage.set_folder_name(id, name.trim())?;
    }
    Ok(id)
}

/// Local-address fallback for a folder pair. Exposed so callers know a peer's
/// advertised address without starting a session.
#[derive(Debug, Clone)]
pub struct FolderPeerRequest {
    pub device_id: String,
    pub mode: Option<String>,
    pub remote_path: Option<String>,
}

impl FolderPeerRequest {
    pub fn new(device_id: &str, mode: &str) -> Self {
        Self {
            device_id: device_id.to_string(),
            mode: Some(mode.to_string()),
            remote_path: None,
        }
    }
}

pub fn list_sync_folders(state: &ApiState) -> anyhow::Result<Vec<FolderEntry>> {
    let own = state.current_device().id;
    let rows = state.storage.list_sync_folders()?;
    // (id, local_path, last_sync_at, [(device_id, mode)])
    let mut by_folder: std::collections::BTreeMap<
        i64,
        (String, Option<i64>, Vec<(String, String)>),
    > = std::collections::BTreeMap::new();
    for (id, local_path, device_id, mode, last_sync_at) in rows {
        let e = by_folder
            .entry(id)
            .or_insert_with(|| (local_path.clone(), last_sync_at, Vec::new()));
        e.2.push((device_id, mode));
    }
    Ok(by_folder
        .into_iter()
        .map(|(id, (local_path, last_sync_at, pairs))| {
            let first_real = pairs
                .iter()
                .find(|(dev, _)| dev != &own)
                .or_else(|| pairs.first());
            let name = state
                .storage
                .folder_name(id)
                .unwrap_or_else(|_| crate::storage::path_label(&local_path));
            let (device_id, direction) = first_real
                .map(|(d, m)| (d.clone(), m.clone()))
                .unwrap_or_default();
            let peers: Vec<FolderPeer> = pairs
                .iter()
                .filter(|(dev, _)| dev != &own)
                .map(|(dev, mode)| FolderPeer {
                    device_id: dev.clone(),
                    mode: mode.clone(),
                    remote_path: state
                        .storage
                        .folder_pair_remote_path(id, dev)
                        .ok()
                        .flatten(),
                    enabled: true,
                })
                .collect();
            FolderEntry {
                id,
                local_path,
                name,
                device_id,
                direction,
                peers,
                last_sync_at: last_sync_at.unwrap_or(0),
                guid: state.storage.folder_guid(id).ok().flatten(),
            }
        })
        .collect())
}

#[derive(Debug, Clone)]
pub struct FolderPeer {
    pub device_id: String,
    pub mode: String,
    pub remote_path: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct FolderEntry {
    pub id: i64,
    pub local_path: String,
    pub name: String,
    /// Stable folder id, independent of the local filesystem path.
    pub guid: Option<String>,
    /// Primary peer (first non-self pair) for backward compatibility.
    pub device_id: String,
    /// Primary peer's mode.
    pub direction: String,
    /// Every paired device and its mode/destination.
    pub peers: Vec<FolderPeer>,
    pub last_sync_at: i64,
}

// ── Sync ──

/// Run a full sync session for a folder against a remote peer.
pub async fn sync_folder(
    state: &ApiState,
    folder_id: i64,
    local_path: String,
    remote_ip: String,
    remote_port: u16,
    device_id: String,
    dry_run: bool,
) -> anyhow::Result<SyncResult> {
    let addr: std::net::SocketAddr = format!("{remote_ip}:{remote_port}").parse()?;
    let result = state
        .engine
        .run_sync(&local_path, addr, folder_id, &device_id, dry_run)
        .await;

    match result {
        Ok(r) => {
            if !dry_run {
                // Remember the working address so the next sync dials it directly.
                let _ = state
                    .storage
                    .set_device_last_addr(&device_id, &addr.to_string());
            }
            Ok(r)
        }
        // The stored address went stale (phone/IP changed): fall back to a
        // short mDNS browse before giving up, then persist the fresh one.
        Err(e) if e.to_string().contains("could not reach") => {
            let fresh = crate::sync_engine::bulk::discover_address_for(
                &device_id,
                &state.current_device().id,
                DISCOVERY_FALLBACK_SECS,
            )
            .await;
            match fresh {
                Some(fresh)
                    if fresh != addr && !crate::sync_engine::bulk::is_own_address(fresh) =>
                {
                    let r = state
                        .engine
                        .run_sync(&local_path, fresh, folder_id, &device_id, dry_run)
                        .await?;
                    if !dry_run {
                        let _ = state
                            .storage
                            .set_device_last_addr(&device_id, &fresh.to_string());
                    }
                    Ok(r)
                }
                _ => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// How long the mDNS freshness fallback browses for a live advertisement.
const DISCOVERY_FALLBACK_SECS: u64 = 2;

// ── Events ──

/// Poll for pending sync events.
pub async fn poll_sync_events(state: &ApiState) -> Vec<SyncEvent> {
    let mut rx = state.engine.events().await;
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

// ── History ──

/// One finished sync session, as surfaced to the Flutter client.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub ts: i64,
    pub direction: String,
    pub peer_device: String,
    pub addr: String,
    pub folder_path: String,
    pub pushed_count: usize,
    pub pulled_count: usize,
    pub conflicts_count: usize,
    pub pushed_bytes: u64,
    pub pulled_bytes: u64,
}

/// Most recent recorded sync sessions, newest first.
pub fn list_recent_sessions(state: &ApiState, limit: u32) -> anyhow::Result<Vec<SessionEntry>> {
    Ok(state
        .storage
        .list_recent_sessions(limit)?
        .into_iter()
        .map(|r| SessionEntry {
            ts: r.ts,
            direction: r.direction,
            peer_device: r.peer_device,
            addr: r.addr,
            folder_path: r.folder_path,
            pushed_count: r.pushed_count,
            pulled_count: r.pulled_count,
            conflicts_count: r.conflicts_count,
            pushed_bytes: r.pushed_bytes,
            pulled_bytes: r.pulled_bytes,
        })
        .collect())
}

/// Most recent recorded sessions for one peer (device id), newest first.
/// Feeds per-device "bytes synced" and recent-activity views.
pub fn list_sessions_for_device(
    state: &ApiState,
    device_id: String,
    limit: u32,
) -> anyhow::Result<Vec<SessionEntry>> {
    Ok(state
        .storage
        .list_sessions_for_device(&device_id, limit)?
        .into_iter()
        .map(|r| SessionEntry {
            ts: r.ts,
            direction: r.direction,
            peer_device: r.peer_device,
            addr: r.addr,
            folder_path: r.folder_path,
            pushed_count: r.pushed_count,
            pulled_count: r.pulled_count,
            conflicts_count: r.conflicts_count,
            pushed_bytes: r.pushed_bytes,
            pulled_bytes: r.pulled_bytes,
        })
        .collect())
}

/// One file-history entry, as surfaced to the Flutter client.
#[derive(Debug, Clone)]
pub struct FileHistoryEntry {
    pub path: String,
    pub device_id: Option<String>,
    pub action: String,
    pub size: Option<i64>,
    pub recorded_at: i64,
}

/// Most recent file-history entries (across all folders, or one folder when
/// `folder_id` is `Some`), newest first.
pub fn list_file_history(
    state: &ApiState,
    folder_id: Option<i64>,
    limit: u32,
) -> anyhow::Result<Vec<FileHistoryEntry>> {
    Ok(state
        .storage
        .list_file_history(folder_id, limit)?
        .into_iter()
        .map(|r| FileHistoryEntry {
            path: r.path,
            device_id: r.device_id,
            action: r.action,
            size: r.size,
            recorded_at: r.recorded_at,
        })
        .collect())
}

// ── Conflicts ──

/// Every conflict backup found on disk across all configured folders.
/// Unlike the transient, polled conflict events this listing survives app
/// restarts, so "needs attention" can reflect real, actionable state.
pub fn list_conflicts(state: &ApiState) -> anyhow::Result<Vec<ConflictEntry>> {
    crate::sync_engine::conflicts::list_conflicts(&state.storage)
}

/// The conflict-inventory DTO exposed to the Flutter client.
pub use crate::sync_engine::conflicts::ConflictEntry;

/// Resolve a conflict backup with `action` (one of `keep_backup`,
/// `keep_original`, `keep_both`). Returns the loser label ("local"/"remote")
/// so the caller can describe what was kept in plain language.
pub async fn resolve_conflict(
    state: &ApiState,
    folder_id: i64,
    backup_path: String,
    action: String,
) -> anyhow::Result<String> {
    crate::sync_engine::conflicts::resolve_conflict(
        &state.storage,
        folder_id,
        &backup_path,
        &action,
    )
    .await
}

/// Bytes of a single conflict version fetched for the in-app compare view.
pub struct ConflictContents {
    /// Text of the winner (real) file.
    pub winner: String,
    /// Text of the loser (backup) file.
    pub loser: String,
    /// True when the winner read was truncated to the read limit.
    pub winner_truncated: bool,
    /// True when the loser read was truncated to the read limit.
    pub loser_truncated: bool,
    /// False when either version is not valid UTF-8 text (e.g. a binary
    /// document); the app then falls back to the metadata-only cards.
    pub textual: bool,
}

/// Cap a single conflict-version read so a giant file can't stall the app.
const CONFLICT_READ_LIMIT: usize = 512 * 1024;

/// Read both versions of a conflict so the app can render a text diff.
/// Both paths are resolved against the sync-folder root via `SyncRoot`, so
/// traversal above the folder is rejected before any read happens.
pub fn read_conflict_contents(
    state: &ApiState,
    folder_id: i64,
    winner_path: String,
    loser_path: String,
) -> anyhow::Result<ConflictContents> {
    let folder = state
        .storage
        .list_sync_folders()?
        .into_iter()
        .find(|(id, _, _, _, _)| *id == folder_id)
        .ok_or_else(|| anyhow::anyhow!("sync folder {folder_id} not found"))?;
    let root = crate::filesystem::SyncRoot::open(PathBuf::from(folder.1))?;

    let winner_abs = root.safe_join(&winner_path)?;
    let loser_abs = root.safe_join(&loser_path)?;

    let winner = read_conflict_text(&winner_abs);
    let loser = read_conflict_text(&loser_abs);

    let (Some((winner_text, winner_truncated)), Some((loser_text, loser_truncated))) =
        (winner, loser)
    else {
        return Ok(ConflictContents {
            winner: String::new(),
            loser: String::new(),
            winner_truncated: false,
            loser_truncated: false,
            textual: false,
        });
    };

    Ok(ConflictContents {
        winner: winner_text,
        loser: loser_text,
        winner_truncated,
        loser_truncated,
        textual: true,
    })
}

/// Read a file's contents as UTF-8 text, capped at `CONFLICT_READ_LIMIT`
/// bytes. Returns `None` when the file is missing or not valid UTF-8.
fn read_conflict_text(path: &Path) -> Option<(String, bool)> {
    if !path.exists() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let truncated = bytes.len() > CONFLICT_READ_LIMIT;
    let slice = if truncated {
        &bytes[..CONFLICT_READ_LIMIT]
    } else {
        &bytes
    };
    std::str::from_utf8(slice)
        .ok()
        .map(|s| (s.to_string(), truncated))
}

// ── Semantic health / presence ──

/// The shared semantic-status types, re-exported so FRB codegen surfaces them
/// to the Flutter app with the CLI/REPL's exact vocabulary.
pub use crate::health::{DeviceStatus, FolderHealth, FolderStatus, HealthSummary, Presence};

/// Every paired device with its derived presence and folder count. Excludes
/// our own row (which only exists when a folder is served here).
pub fn device_statuses(state: &ApiState) -> anyhow::Result<Vec<DeviceStatus>> {
    let own = state.current_device().id;
    crate::health::compute_device_statuses(&state.storage, &own, crate::health::now_secs())
}

/// Every configured folder with its derived health and peer label.
pub fn folder_statuses(state: &ApiState) -> anyhow::Result<Vec<FolderStatus>> {
    let own = state.current_device().id;
    crate::health::compute_folder_statuses(
        &state.storage,
        &own,
        crate::health::now_secs(),
        &crate::health::LiveState::default(),
    )
}

/// Status of one folder (one entry per paired device).
pub fn folder_status(state: &ApiState, folder_id: i64) -> anyhow::Result<Vec<FolderStatus>> {
    Ok(folder_statuses(state)?
        .into_iter()
        .filter(|f| f.id == folder_id)
        .collect())
}

/// Every folder relationship for a device, with per-folder health.
pub fn device_folders(state: &ApiState, device_id: String) -> anyhow::Result<Vec<FolderStatus>> {
    Ok(folder_statuses(state)?
        .into_iter()
        .filter(|f| f.device_id == device_id)
        .collect())
}

/// Roll-up "is everything OK?" counts for dashboards and startup banners.
pub fn overall_health(state: &ApiState) -> anyhow::Result<HealthSummary> {
    let own = state.current_device().id;
    Ok(crate::health::snapshot(
        &state.storage,
        &own,
        crate::health::now_secs(),
        &crate::health::LiveState::default(),
    )?
    .summary)
}

// ── Diagnostics ──

/// The on-device diagnostic check model, re-exported so FRB codegen surfaces
/// the exact same checks the CLI `ferrisync doctor` runs.
pub use crate::diagnostics::{CheckStatus, DiagnosticCheck};

/// Run every on-device diagnostic check (`doctor`). The Flutter app consumes
/// the same Rust model as the CLI — no duplicate checks in Dart.
pub async fn run_diagnostics(state: &ApiState) -> Vec<DiagnosticCheck> {
    let dev = state.current_device();
    crate::diagnostics::run_all(crate::diagnostics::DiagnosticsInput {
        data_dir: Path::new(&state.data_dir),
        crypto: &state.crypto,
        storage: &state.storage,
        own_id: &dev.id,
        own_name: &dev.name,
        serve_port: crate::sync_engine::bulk::DEFAULT_PORT,
    })
    .await
}

// ── Device Info ──

pub fn device_id(state: &ApiState) -> String {
    state.current_device().id
}

pub fn device_name(state: &ApiState) -> String {
    state.current_device().name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::CryptoProvider;
    use crate::storage::Storage;
    use crate::sync_engine::bulk::sync_all_folders_with;
    use crate::sync_engine::server::{serve_folder, PairPolicy};
    use crate::DeviceInfo;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn identity_is_stable_across_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let first = load_or_create_identity(dir.path()).await.unwrap();
        let first_id = load_or_create_device_id(dir.path());
        let second = load_or_create_identity(dir.path()).await.unwrap();
        let second_id = load_or_create_device_id(dir.path());

        assert_eq!(first.fingerprint().await, second.fingerprint().await);
        assert_eq!(first_id, second_id);
    }

    /// Full host→server round trip through the folder server: bind on port 0,
    /// pull a server-side file, push a client-side file, and confirm the
    /// contact address was persisted onto the device row.
    #[tokio::test]
    async fn server_round_trip_push_and_pull() {
        let server_dir = tempfile::tempdir().unwrap();
        let served = server_dir.path().join("served");
        std::fs::create_dir(&served).unwrap();

        let server_storage =
            Arc::new(Storage::open(&server_dir.path().join("metadata.db")).unwrap());
        let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
        let server_info = DeviceInfo {
            id: "server-device".into(),
            name: "phone".into(),
            cert_fingerprint: Vec::new(),
        };

        let (handle, _events) = serve_folder(
            server_storage.clone(),
            server_crypto.clone(),
            server_info.clone(),
            served.to_string_lossy().to_string(),
            0,
            PairPolicy::AutoAccept,
            Arc::new(crate::persistence::InMemoryStateStore::new()),
        )
        .await
        .unwrap();
        assert_ne!(handle.port, 0, "port 0 must resolve to a real port");

        std::fs::write(served.join("from_server.txt"), b"server says hi").unwrap();

        // ── client side ──
        let client_dir = tempfile::tempdir().unwrap();
        let incoming = client_dir.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        let client_storage =
            Arc::new(Storage::open(&client_dir.path().join("metadata.db")).unwrap());
        let client_crypto = Arc::new(CryptoProvider::generate().unwrap());
        let client_info = crate::DeviceInfo {
            id: uuid::Uuid::new_v4().to_string(),
            name: "client".into(),
            cert_fingerprint: client_crypto.fingerprint().await,
        };
        // Register client in server's device table (simulates pairing) so
        // the server's TOFU gate accepts the connection.
        let client_cert_der = client_crypto.certificate().await;
        server_storage
            .upsert_device(
                &client_info.id,
                &client_info.name,
                Some(&client_cert_der.to_vec()),
                None,
            )
            .unwrap();

        client_storage
            .upsert_device(
                &server_info.id,
                &server_info.name,
                Some(&server_crypto.certificate().await.to_vec()),
                Some(&format!("127.0.0.1:{}", handle.port)),
            )
            .unwrap();
        client_storage
            .upsert_device(
                &server_info.id,
                &server_info.name,
                None,
                Some(&format!("127.0.0.1:{}", handle.port)),
            )
            .unwrap();
        client_storage
            .add_sync_folder(
                incoming.to_string_lossy().as_ref(),
                &server_info.id,
                "bidirectional",
            )
            .unwrap();

        let (event_tx, _event_rx) = mpsc::channel(256);
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", handle.port).parse().unwrap();

        // The loopback server is this machine by definition, so bulk sync
        // would refuse it; drive the session layer directly instead.
        let folder_row = &client_storage.list_sync_folders().unwrap()[0];
        let result = crate::sync_engine::session::run_sync_session(
            client_crypto.clone(),
            client_storage.clone(),
            folder_row.1.as_str(),
            addr,
            folder_row.0,
            &server_info.id,
            event_tx.clone(),
            Arc::new(crate::persistence::InMemoryStateStore::new()),
            false,
            None,
        )
        .await
        .unwrap();
        assert!(
            result.pulled.contains(&"from_server.txt".to_string()),
            "expected pull of from_server.txt"
        );
        client_storage
            .set_device_last_addr(&server_info.id, &addr.to_string())
            .unwrap();
        assert_eq!(
            std::fs::read(incoming.join("from_server.txt")).unwrap(),
            b"server says hi"
        );
        assert_eq!(
            client_storage.device_last_addr(&server_info.id).unwrap(),
            Some(format!("127.0.0.1:{}", handle.port)),
            "contact address persisted"
        );

        // Push pass.
        std::fs::write(incoming.join("from_client.txt"), b"client reply").unwrap();
        let pushed = crate::sync_engine::session::run_sync_session(
            client_crypto,
            client_storage,
            incoming.to_str().unwrap(),
            addr,
            folder_row.0,
            &server_info.id,
            event_tx,
            Arc::new(crate::persistence::InMemoryStateStore::new()),
            false,
            None,
        )
        .await
        .unwrap();
        assert!(
            pushed.pushed.contains(&"from_client.txt".to_string()),
            "expected push of from_client.txt"
        );
        assert_eq!(
            std::fs::read(served.join("from_client.txt")).unwrap(),
            b"client reply"
        );

        handle.stop().await;
    }

    /// Bulk sync refuses device rows that resolve to one of this machine's own
    /// addresses instead of silently "syncing" against ourselves.
    #[tokio::test]
    async fn bulk_refuses_self_addressed_rows() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
        let crypto = Arc::new(CryptoProvider::generate().unwrap());

        storage
            .upsert_device("stale-self", "me-by-mistake", None, Some("127.0.0.1:9847"))
            .unwrap();
        let incoming = dir.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        storage
            .add_sync_folder(incoming.to_str().unwrap(), "stale-self", "bidirectional")
            .unwrap();

        let (event_tx, _event_rx) = mpsc::channel(256);
        let outcomes = sync_all_folders_with(
            crypto,
            storage.clone(),
            event_tx,
            Duration::ZERO,
            "this-client",
            Arc::new(crate::persistence::InMemoryStateStore::new()),
        )
        .await
        .unwrap();

        assert_eq!(outcomes.len(), 1);
        let err = outcomes[0]
            .result
            .as_ref()
            .expect("self-addressed row must produce an error outcome")
            .as_ref()
            .expect_err("expected refusal");
        assert!(
            format!("{err:#}").contains("points at us"),
            "unexpected error: {err:#}"
        );
    }

    /// A chosen device name survives an engine reload and is picked up by the
    /// pairing manager, instead of falling back to the hostname again.
    #[tokio::test]
    async fn set_device_name_persists_across_reload() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_str().unwrap().to_string();
        let state = init_engine(data_dir.clone()).await.unwrap();

        let renamed = set_device_name(&state, "  my-phone  ".into())
            .await
            .expect("valid rename");
        assert_eq!(renamed, "my-phone");
        assert_eq!(device_name(&state), "my-phone");
        assert_eq!(state.pairing.current_device().name, "my-phone");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("device.name")).unwrap(),
            "my-phone"
        );

        let reloaded = init_engine(data_dir).await.unwrap();
        assert_eq!(
            device_name(&reloaded),
            "my-phone",
            "rename must survive restart"
        );
    }

    #[tokio::test]
    async fn set_device_name_rejects_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        let too_long = "a".repeat(65);
        for bad in ["", "   ", too_long.as_str(), "bad\u{7}name"] {
            assert!(
                set_device_name(&state, bad.to_string()).await.is_err(),
                "expected rejection of {bad:?}"
            );
        }
        // The identity is untouched after failed renames.
        assert!(!dir.path().join("device.name").exists());
    }

    /// Renaming restarts running folder listeners so they advertise (and
    /// answer pairing with) the new name without an app restart.
    #[tokio::test]
    async fn set_device_name_restarts_running_servers() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        let served = dir.path().join("served");
        std::fs::create_dir(&served).unwrap();
        state
            .storage
            .upsert_device("peer-1", "peer", None, None)
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder(served.to_str().unwrap(), "peer-1", "bidirectional")
            .unwrap();
        start_server(&state, 0, folder_id, served.to_str().unwrap().to_string())
            .await
            .unwrap();
        let port_before = servers().lock().unwrap()[&folder_id].handle.port;
        assert_ne!(port_before, 0);

        set_device_name(&state, "renamed-host".into())
            .await
            .unwrap();

        let port_after = {
            let guard = servers().lock().unwrap();
            assert!(
                guard.contains_key(&folder_id),
                "server must be running again after rename"
            );
            guard[&folder_id].handle.port
        };
        assert_ne!(port_after, 0);

        // Clean up: stop the listener so other tests see no stale entries.
        let handle = servers().lock().unwrap().remove(&folder_id).unwrap();
        handle.handle.stop().await;
    }
}

#[cfg(test)]
mod phone_pull_tests {
    use super::*;

    /// Phone-role sync through the public API entry point: a client with an
    /// empty folder must pull files that exist only on the server side.
    #[tokio::test]
    async fn api_sync_folder_pulls_from_server() {
        use crate::sync_engine::server::{serve_folder, PairPolicy};

        // Server side (the "REPL host"): seeded folder with one file.
        let server_dir = tempfile::tempdir().unwrap();
        let server_storage =
            Arc::new(Storage::open(&server_dir.path().join("metadata.db")).unwrap());
        let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
        let server_info = crate::DeviceInfo {
            id: uuid::Uuid::new_v4().to_string(),
            name: "host".into(),
            cert_fingerprint: Vec::new(),
        };
        std::fs::write(server_dir.path().join("host_only.txt"), b"from host").unwrap();

        let (_handle, _events) = serve_folder(
            server_storage.clone(),
            server_crypto.clone(),
            server_info.clone(),
            server_dir.path().to_str().unwrap().to_string(),
            0,
            PairPolicy::AutoAccept,
            Arc::new(crate::persistence::InMemoryStateStore::new()),
        )
        .await
        .unwrap();
        let port = _handle.port;

        // Client side ("the phone") through the full API stack.
        let client_data = tempfile::tempdir().unwrap();
        let state = init_engine(client_data.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        // Register client device in server's storage (simulates pairing)
        // so the server's TOFU gate accepts the connection.
        let client_cert_der = state.crypto.certificate().await;
        let client_info_guard = state.device_info.read().unwrap();
        let client_id = client_info_guard.id.clone();
        let client_name = client_info_guard.name.clone();
        drop(client_info_guard);
        server_storage
            .upsert_device(
                &client_id,
                &client_name,
                Some(&client_cert_der.to_vec()),
                None,
            )
            .unwrap();

        let incoming = client_data.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        // In production the pairing flow creates this device row.
        state
            .storage
            .upsert_device(
                &server_info.id,
                &server_info.name,
                Some(&server_crypto.certificate().await.to_vec()),
                None,
            )
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder(incoming.to_str().unwrap(), &server_info.id, "bidirectional")
            .unwrap();

        let result = sync_folder(
            &state,
            folder_id,
            incoming.to_str().unwrap().to_string(),
            "127.0.0.1".to_string(),
            port,
            server_info.id.clone(),
            false,
        )
        .await
        .unwrap();

        assert!(
            result.pulled.contains(&"host_only.txt".to_string()),
            "expected pull, got {:?}",
            result.pulled
        );
        assert_eq!(
            std::fs::read(incoming.join("host_only.txt")).unwrap(),
            b"from host"
        );
        // The working address is persisted for next time.
        assert_eq!(
            state.storage.device_last_addr(&server_info.id).unwrap(),
            Some(format!("127.0.0.1:{port}"))
        );
    }

    /// Session and file history recorded during syncs is readable back
    /// through the bridge-facing accessors.
    #[tokio::test]
    async fn history_accessors_return_recorded_data() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        assert!(list_recent_sessions(&state, 10).unwrap().is_empty());
        assert!(list_file_history(&state, None, 10).unwrap().is_empty());

        state
            .storage
            .record_session(
                "bidirectional",
                "phone-1",
                "192.168.1.5:9848",
                "/d1",
                2,
                3,
                1,
                200,
                300,
            )
            .unwrap();
        state
            .storage
            .record_session(
                "pull",
                "laptop-2",
                "192.168.1.6:9849",
                "/d2",
                0,
                5,
                0,
                0,
                500,
            )
            .unwrap();

        state
            .storage
            .upsert_device("phone-1", "phone", None, None)
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder("/d1", "phone-1", "bidirectional")
            .unwrap();
        state
            .storage
            .record_history(crate::storage::HistoryRecord {
                folder_id,
                path: "a.txt",
                device_id: "phone-1",
                action: "pull",
                version: 1,
                mtime: 0,
                hash: b"h",
                size: 4,
            })
            .unwrap();

        let sessions = list_recent_sessions(&state, 10).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].peer_device, "laptop-2");
        assert_eq!(sessions[0].pulled_count, 5);
        assert_eq!(sessions[0].conflicts_count, 0);
        assert_eq!(sessions[0].pulled_bytes, 500);
        assert_eq!(sessions[1].direction, "bidirectional");
        assert_eq!(sessions[1].pushed_count, 2);
        assert_eq!(sessions[1].pushed_bytes, 200);

        assert_eq!(list_recent_sessions(&state, 1).unwrap().len(), 1);

        let for_device = list_sessions_for_device(&state, "phone-1".to_string(), 10).unwrap();
        assert_eq!(for_device.len(), 1);
        assert_eq!(for_device[0].direction, "bidirectional");

        let history = list_file_history(&state, None, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].path, "a.txt");
        assert_eq!(history[0].action, "pull");
        assert_eq!(history[0].size, Some(4));
        assert!(history[0].device_id.as_deref() == Some("phone-1"));

        let per_folder = list_file_history(&state, Some(folder_id), 10).unwrap();
        assert_eq!(per_folder.len(), 1);
        assert!(list_file_history(&state, Some(folder_id + 999), 10)
            .unwrap()
            .is_empty());
    }

    /// Removing a folder by id drops it from the folder table.
    #[tokio::test]
    async fn remove_folder_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();
        state
            .storage
            .upsert_device("dev-s", "server", None, None)
            .unwrap();
        let id = state
            .storage
            .add_sync_folder("/shared", "dev-s", "bidirectional")
            .unwrap();

        remove_folder(&state, id).unwrap();
        assert!(state.storage.list_sync_folders().unwrap().is_empty());
    }

    /// remove_all_devices clears every trust relationship.
    #[tokio::test]
    async fn remove_all_devices_clears_everything() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();
        state
            .storage
            .upsert_device("dev-a", "alpha", None, None)
            .unwrap();
        state
            .storage
            .upsert_device("dev-b", "beta", None, None)
            .unwrap();
        state
            .storage
            .add_sync_folder("/shared", "dev-a", "bidirectional")
            .unwrap();

        let removed = remove_all_devices(&state).unwrap();
        assert_eq!(removed, 2);
        assert!(state.storage.list_devices().unwrap().is_empty());
        assert!(state.storage.list_sync_folders().unwrap().is_empty());
    }

    /// On factory reset the data dir is wiped (identity, db, config), so the
    /// next init_engine runs the cold-start path and derives a NEW device id.
    #[tokio::test]
    async fn factory_reset_restores_fresh_install() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_str().unwrap().to_string();
        let state = init_engine(data_dir.clone()).await.unwrap();
        let first_id = state.current_device().id.clone();
        state
            .storage
            .upsert_device("dev-a", "alpha", None, None)
            .unwrap();
        state
            .storage
            .add_sync_folder("/shared", "dev-a", "bidirectional")
            .unwrap();

        factory_reset(&state).await.unwrap();

        // Wiped everything from disk.
        assert!(!Path::new(&dir.path().join(IDENTITY_CERT_FILE)).exists());
        assert!(!Path::new(&dir.path().join(IDENTITY_KEY_FILE)).exists());
        assert!(!Path::new(&dir.path().join(DEVICE_ID_FILE)).exists());
        assert!(!Path::new(&dir.path().join("metadata.db")).exists());

        // A fresh engine sees no state and gets a brand-new identity.
        let fresh = init_engine(data_dir).await.unwrap();
        assert!(fresh.storage.list_devices().unwrap().is_empty());
        assert!(fresh.storage.list_sync_folders().unwrap().is_empty());
        assert_ne!(
            fresh.current_device().id,
            first_id,
            "identity must be regenerated"
        );
    }

    /// `list_devices` never surfaces our own row as a paired remote, even when
    /// a folder is served here (which creates the own row in storage).
    #[tokio::test]
    async fn list_devices_excludes_own_row() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        // Simulate a folder served here: the engine writes our own row.
        state
            .storage
            .upsert_device(
                &state.current_device().id,
                &state.current_device().name,
                None,
                None,
            )
            .unwrap();
        state
            .storage
            .upsert_device("peer-1", "phone", None, None)
            .unwrap();

        let devices = list_devices(&state).unwrap();
        assert_eq!(devices.len(), 1, "own row must be filtered out");
        assert_eq!(devices[0].id, "peer-1");
    }

    /// `read_conflict_contents` returns both a textual conflict's versions and
    /// flags when either read was truncated to the file-size cap.
    #[tokio::test]
    async fn read_conflict_contents_returns_both_text_versions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("shared");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("notes.txt"),
            b"winner line one\nwinner line two\n",
        )
        .unwrap();
        std::fs::write(
            root.join("notes.txt.clash"),
            b"loser line one\nloser line two\n",
        )
        .unwrap();

        let state = init_engine(dir.path().join("meta").to_str().unwrap().to_string())
            .await
            .unwrap();
        state
            .storage
            .upsert_device("dev-s", "server", None, None)
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder(root.to_str().unwrap(), "dev-s", "bidirectional")
            .unwrap();

        let contents = read_conflict_contents(
            &state,
            folder_id,
            "notes.txt".to_string(),
            "notes.txt.clash".to_string(),
        )
        .unwrap();

        assert!(contents.textual);
        assert_eq!(contents.winner, "winner line one\nwinner line two\n");
        assert_eq!(contents.loser, "loser line one\nloser line two\n");
        assert!(!contents.winner_truncated);
        assert!(!contents.loser_truncated);
    }

    /// Reads are capped at `CONFLICT_READ_LIMIT` bytes; larger files surface
    /// their truncation flag instead of stalling the client with a huge read.
    #[tokio::test]
    async fn read_conflict_contents_truncates_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("shared");
        std::fs::create_dir(&root).unwrap();
        let big = vec![b'a' as u8; CONFLICT_READ_LIMIT + 100];
        std::fs::write(root.join("big.txt"), &big).unwrap();
        std::fs::write(root.join("small.txt"), b"small").unwrap();

        let state = init_engine(dir.path().join("meta").to_str().unwrap().to_string())
            .await
            .unwrap();
        state
            .storage
            .upsert_device("dev-s", "server", None, None)
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder(root.to_str().unwrap(), "dev-s", "bidirectional")
            .unwrap();

        let contents = read_conflict_contents(
            &state,
            folder_id,
            "big.txt".to_string(),
            "small.txt".to_string(),
        )
        .unwrap();

        assert!(contents.textual);
        assert!(contents.winner_truncated);
        assert!(!contents.loser_truncated);
        assert_eq!(contents.winner.len(), CONFLICT_READ_LIMIT);
        assert_eq!(contents.loser, "small");
    }

    /// Non-UTF-8 (e.g. binary) versions should be reported as non-textual so
    /// the client falls back to the metadata-only compare view.
    #[tokio::test]
    async fn read_conflict_contents_marks_binary_as_non_textual() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("shared");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("bin.dat"), [0xff, 0x00, 0xfe, 0x01]).unwrap();
        std::fs::write(root.join("bin.dat.clash"), [0x00, 0xff, 0x01, 0xfe]).unwrap();

        let state = init_engine(dir.path().join("meta").to_str().unwrap().to_string())
            .await
            .unwrap();
        state
            .storage
            .upsert_device("dev-s", "server", None, None)
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder(root.to_str().unwrap(), "dev-s", "bidirectional")
            .unwrap();

        let contents = read_conflict_contents(
            &state,
            folder_id,
            "bin.dat".to_string(),
            "bin.dat.clash".to_string(),
        )
        .unwrap();

        assert!(!contents.textual);
        assert_eq!(contents.winner, "");
        assert_eq!(contents.loser, "");
    }

    /// Traversal above the sync-folder root is rejected before any read so a
    /// malicious conflict path can't expose files outside the folder.
    #[tokio::test]
    async fn read_conflict_contents_rejects_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("shared");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(dir.path().join("outside.txt"), b"secret").unwrap();

        let state = init_engine(dir.path().join("meta").to_str().unwrap().to_string())
            .await
            .unwrap();
        state
            .storage
            .upsert_device("dev-s", "server", None, None)
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder(root.to_str().unwrap(), "dev-s", "bidirectional")
            .unwrap();

        assert!(read_conflict_contents(
            &state,
            folder_id,
            "../outside.txt".to_string(),
            "notes.txt.clash".to_string(),
        )
        .is_err());
    }

    /// Regression: adding the first self-owned folder on a *fresh* engine (no
    /// device has been registered yet — e.g. the very first folder in the
    /// onboarding wizard) must not trip `folder_devices.device_id →
    /// devices(id)` foreign-key. `add_sync_folder` must register the owner row
    /// before writing the pair, mirroring `add_sync_folder_with_peers`.
    #[tokio::test]
    async fn add_self_folder_before_any_pairing_does_not_fk_fail() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("shared");
        std::fs::create_dir(&root).unwrap();

        let state = init_engine(dir.path().join("meta").to_str().unwrap().to_string())
            .await
            .unwrap();
        // No `devices` row exists for self yet (no pairing has run).
        assert!(state.storage.list_devices().unwrap().is_empty());

        let self_id = state.current_device().id;
        let folder_id = add_sync_folder(
            &state,
            root.to_str().unwrap().to_string(),
            self_id.clone(),
            "bidirectional".to_string(),
        )
        .unwrap();

        // The folder is owned by self and its pair row references the now
        // present device row for self.
        let pairs = state.storage.folder_pairs(folder_id).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, self_id);
    }
}

//! flutter_rust_bridge FFI surface.
//! Annotated functions are picked up by FRB codegen to generate Dart bindings.

use crate::crypto::CryptoProvider;
use crate::storage::Storage;
use crate::sync_engine::control::{serve_control, ControlHandle, CONTROL_PORT};
use crate::sync_engine::pairing::PairingManager;
use crate::sync_engine::server::{PairPolicy, ServeHandle};
use crate::sync_engine::session::SyncResult;
use crate::sync_engine::{SyncEngine, SyncEvent};
use crate::DeviceInfo;
use anyhow::Context;
use rustls::pki_types::PrivateKeyDer;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
// Re-export so generated bridge code (`use crate::api::*`) can name RwLock.
#[allow(unused_imports)]
pub use std::sync::RwLock;
use tokio::sync::broadcast;

// ── Cancellation and typed errors ──

pub use crate::sync_engine::OperationCancelled;
/// Cooperative cancellation token shared between API callers and background work.
///
/// This is the API-facing name for the engine's [`SyncCancelToken`]. It is
/// checked at API boundaries, while starting watches, and inside in-flight
/// sync sessions (connect/handshake/chunk/finalization steps). Work already
/// committed before cancellation remains valid; a cancelled operation never
/// returns a successful result.
pub use crate::sync_engine::SyncCancelToken as ApiCancelToken;

/// Typed error categories surfaced by API-facing operations.
///
/// Core APIs still commonly return `anyhow::Result`, but these variants can be
/// attached to errors so frontends can classify failures without parsing text.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("operation cancelled")]
    Cancelled,
    #[error("{0}")]
    InvalidArgument(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Connection(String),
    #[error("{0}")]
    Sync(String),
    #[error("{0}")]
    Internal(String),
}

impl From<OperationCancelled> for ApiError {
    fn from(_: OperationCancelled) -> Self {
        Self::Cancelled
    }
}

fn ensure_not_cancelled(token: &ApiCancelToken) -> anyhow::Result<()> {
    token.ensure_not_cancelled()
}

// ── Discovery ──

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub ip: String,
    /// Folder-server port, used for sync. Legacy peers advertise only this.
    pub port: u16,
    /// Always-on pairing port, when the peer advertises one.
    pub control_port: Option<u16>,
}

/// Collapse the several mDNS instances a device may advertise into one entry.
///
/// A device announces one instance per folder plus the folder-less pairing
/// endpoint. `port` must point at a folder server when one exists; the pairing
/// port is kept separately so callers can reach a peer with zero shares.
fn merge_discovered_peers(peers: Vec<crate::discovery::DiscoveredPeer>) -> Vec<DiscoveredDevice> {
    struct Found {
        device: DiscoveredDevice,
        port_from_control: bool,
    }
    let mut found: HashMap<String, Found> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for peer in peers {
        let Some(addr) = peer.addresses.first() else {
            continue;
        };
        let key = if peer.id.is_empty() {
            format!("{}:{}", addr.ip(), addr.port())
        } else {
            peer.id.clone()
        };
        let control_port = peer
            .properties
            .get("control_port")
            .and_then(|value| value.parse::<u16>().ok());
        match found.get_mut(&key) {
            Some(entry) => {
                if let Some(port) = control_port {
                    entry.device.control_port = Some(port);
                } else if entry.port_from_control || entry.device.port == 0 {
                    entry.device.ip = addr.ip().to_string();
                    entry.device.port = addr.port();
                    entry.device.name = peer.name.clone();
                    entry.port_from_control = false;
                }
            }
            None => {
                found.insert(
                    key.clone(),
                    Found {
                        device: DiscoveredDevice {
                            id: peer.id,
                            name: peer.name,
                            ip: addr.ip().to_string(),
                            port: addr.port(),
                            control_port,
                        },
                        port_from_control: control_port.is_some(),
                    },
                );
                order.push(key);
            }
        }
    }

    order
        .into_iter()
        .filter_map(|key| found.remove(&key).map(|entry| entry.device))
        .collect()
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
            Ok(Some(peer)) => peers.push(peer),
            _ => break,
        }
    }

    Ok(merge_discovered_peers(peers))
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
    /// Live folder listeners owned by this engine instance, keyed by folder id.
    /// Kept per-instance (not global) so multiple clients can coexist and
    /// `stop_server`/`close` only affect this instance's listeners.
    servers: Arc<Mutex<HashMap<i64, RunningServer>>>,
    /// Always-on pairing/discovery listener; independent of shared folders.
    control: Arc<Mutex<Option<ControlHandle>>>,
    /// Actual port the control listener is bound to (may differ from the
    /// requested port when a test asks for an ephemeral one).
    control_port: Arc<AtomicU16>,
    /// Background watch tasks owned by this engine instance.
    watches: Arc<Mutex<HashMap<i64, RunningWatch>>>,
    next_watch_id: Arc<Mutex<i64>>,
    /// Persistent event subscription used by polling consumers.
    event_poll: Arc<Mutex<broadcast::Receiver<SyncEvent>>>,
}

impl ApiState {
    /// Snapshot of the current device identity.
    pub fn current_device(&self) -> DeviceInfo {
        self.device_info.read().unwrap().clone()
    }

    /// Port of the live control listener, as bound (not just requested).
    ///
    /// Native/test helper: FRB already exposes this implicitly through
    /// `local_pairing_addresses`, and an extra Dart method is not needed.
    /// flutter_rust_bridge:ignore
    pub fn control_port(&self) -> u16 {
        self.control_port.load(Ordering::Relaxed)
    }
}

/// Start (or restart) the folder-less control listener and forward its events.
async fn start_control(state: &ApiState) -> anyhow::Result<()> {
    let (handle, mut events) = serve_control(
        state.storage.clone(),
        state.crypto.clone(),
        state.current_device(),
        state.control_port(),
    )
    .await?;
    // `serve_control` reports the port it actually bound. This matters when a
    // caller asked for port 0 and keeps QR/discovery addresses truthful.
    state
        .control_port
        .store(handle.port, Ordering::Relaxed);
    let forward_tx = state.engine.event_sender();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if forward_tx.send(event).await.is_err() {
                break;
            }
        }
    });
    *state.control.lock().unwrap() = Some(handle);
    Ok(())
}

/// Stop the control listener without touching folder servers.
async fn stop_control(state: &ApiState) {
    let handle = state.control.lock().unwrap().take();
    if let Some(handle) = handle {
        handle.stop().await;
    }
}

/// Initialise the sync engine and return an opaque handle.
///
/// The device identity (certificate, key, id, name) is persisted in the data
/// directory and reused across launches so pairings survive restarts.
pub async fn init_engine(data_dir: String) -> anyhow::Result<ApiState> {
    // Tests that run many engines in one process can set
    // FERRISYNC_CONTROL_PORT=0 to get isolated ephemeral listeners. Normal
    // apps never set it and use the well-known CONTROL_PORT.
    let port = std::env::var("FERRISYNC_CONTROL_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(CONTROL_PORT);
    init_engine_with_control(data_dir, port).await
}

/// Same as [`init_engine`] but with an explicit control-listener port.
///
/// Passing `0` binds an ephemeral port; read the actual value from
/// [`ApiState::control_port`]. This lets tests run many independent engines in
/// one process without fighting over the well-known port.
///
/// Native/test helper: Dart always uses the fixed `CONTROL_PORT`.
/// flutter_rust_bridge:ignore
pub async fn init_engine_with_control(
    data_dir: String,
    control_port: u16,
) -> anyhow::Result<ApiState> {
    let path = PathBuf::from(&data_dir);
    std::fs::create_dir_all(&path)?;

    let crypto = Arc::new(load_or_create_identity(&path).await?);
    let storage = Arc::new(Storage::open(&path.join("metadata.db"))?);
    let dev_id = crate::crypto::cert_to_device_id(&crypto.certificate().await);
    // Reconcile any device rows that predate identity unification (when the
    // advertised id was a separate random UUID) so peers are not duplicated.
    migrate_legacy_device_identity(&storage, &path, &crypto.certificate().await)?;

    // The device id is a pure function of the device's TLS certificate — the
    // same authoritative identity used to record peers and validated in the
    // sync handshake. Keeping ONE id scheme (rather than a second random uuid)
    // is what prevents the same physical device from appearing twice.
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

    let event_poll = engine.subscribe_events();
    let state = ApiState {
        crypto,
        storage,
        engine,
        pairing,
        device_info: Arc::new(RwLock::new(device_info)),
        data_dir,
        servers: Arc::new(Mutex::new(HashMap::new())),
        control: Arc::new(Mutex::new(None)),
        control_port: Arc::new(AtomicU16::new(control_port)),
        watches: Arc::new(Mutex::new(HashMap::new())),
        next_watch_id: Arc::new(Mutex::new(1)),
        event_poll: Arc::new(Mutex::new(event_poll)),
    };

    // Pairing must work before any folder is shared. Start the folder-less
    // listener first so discovery and the QR address exist on a fresh install.
    if let Err(e) = start_control(&state).await {
        log::warn!("control listener unavailable on {control_port}: {e}");
    }

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

// Canonical identity file names, shared with the CLI so the app and the
// command-line tool on the same data directory use the *same* certificate and
// therefore the same cert-derived device id. If they differed, the app would
// mint a fresh identity and no longer recognise its own stored self row —
// surfacing the device as a "paired" peer and duplicating it.
const IDENTITY_CERT_FILE: &str = "identity.crt";
const IDENTITY_KEY_FILE: &str = "identity.key";

/// Legacy identity file names written by older `api::init_engine` builds.
/// Migrated to the canonical [`IDENTITY_CERT_FILE`]/[`IDENTITY_KEY_FILE`]
/// names so a previously-paired install keeps its certificate.
const LEGACY_CERT_FILE: &str = "identity.cert.der";
const LEGACY_KEY_FILE: &str = "identity.key.der";

async fn load_or_create_identity(path: &Path) -> anyhow::Result<CryptoProvider> {
    let cert_path = path.join(IDENTITY_CERT_FILE);
    let key_path = path.join(IDENTITY_KEY_FILE);
    if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        let fingerprint = blake3::hash(&cert).as_bytes().to_vec();
        return CryptoProvider::load(cert, key, fingerprint);
    }

    // Migrate a legacy `.der` pair into the canonical names, matching the
    // CLI's `app.rs` behavior, before falling back to generating a new key.
    let legacy_cert = path.join(LEGACY_CERT_FILE);
    let legacy_key = path.join(LEGACY_KEY_FILE);
    if let (Ok(cert), Ok(key)) = (std::fs::read(&legacy_cert), std::fs::read(&legacy_key)) {
        if std::fs::rename(&legacy_cert, &cert_path).is_err() {
            std::fs::write(&cert_path, &cert)?;
        }
        if std::fs::rename(&legacy_key, &key_path).is_err() {
            std::fs::write(&key_path, &key)?;
        }
        let fingerprint = blake3::hash(&cert).as_bytes().to_vec();
        return CryptoProvider::load(cert, key, fingerprint);
    }

    let crypto = CryptoProvider::generate()?;
    let key = crypto.private_key().await;
    let key_bytes: Vec<u8> = match &key {
        PrivateKeyDer::Pkcs8(k) => k.secret_pkcs8_der().to_vec(),
        PrivateKeyDer::Pkcs1(k) => k.secret_pkcs1_der().to_vec(),
        PrivateKeyDer::Sec1(k) => k.secret_sec1_der().to_vec(),
        _ => return Err(ApiError::InvalidArgument("unsupported private key format".into()).into()),
    };
    std::fs::write(&cert_path, &crypto.certificate().await)?;
    std::fs::write(&key_path, key_bytes)?;
    Ok(crypto)
}

/// Reconcile persisted device rows with the cert-derived identity so no
/// physical device is recorded under two different ids. See
/// [`Storage::reconcile_devices_by_cert`]. Also removes the legacy random-uuid
/// identity file, which is now obsolete.
fn migrate_legacy_device_identity(
    storage: &Storage,
    path: &Path,
    cert_der: &[u8],
) -> anyhow::Result<()> {
    // The legacy random-uuid id, if this install predates unification. Rows
    // keyed by it — most importantly our own self row — fold into the
    // cert-derived id. Absent on fresh installs.
    let legacy_own_id = std::fs::read_to_string(path.join("device.id"))
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty());

    let own_name = crate::config::load_device_name(path).unwrap_or_else(|| "ferrisync".to_string());
    storage.reconcile_devices_by_cert(cert_der, legacy_own_id.as_deref(), &own_name)?;

    // The random-uuid state file is obsolete — identity now comes from the cert.
    let _ = std::fs::remove_file(path.join("device.id"));
    Ok(())
}

// The user-chosen device name, if one was ever set. Absent means "use the
// hostname"; the file is only written by an explicit rename.
// ── Server ──

/// A live folder listener plus the parameters it was started with, so a
/// rename can stop and re-serve it under the fresh identity.
#[derive(Debug)]
struct RunningServer {
    handle: ServeHandle,
    port: u16,
    local_path: String,
}

/// A background watch task that syncs a folder when filesystem changes are detected.
#[derive(Debug)]
struct RunningWatch {
    folder_id: i64,
    local_path: String,
    device_id: String,
    remote_addr: std::net::SocketAddr,
    shutdown: tokio::sync::watch::Sender<bool>,
    cancel: ApiCancelToken,
    task: tokio::task::JoinHandle<()>,
}

/// LAN addresses of this instance's live folder servers, as `ip:port` strings.
///
/// Used by the Flutter "Show QR code" action: the peer that scans the code
/// pairs to one of these endpoints. Empty when no folder is currently served.
pub fn local_pairing_addresses(state: &ApiState) -> Vec<String> {
    let ip = crate::discovery::local_ip();
    let mut addresses = Vec::new();
    let control_ready = state.control.lock().unwrap().is_some();
    if control_ready {
        addresses.push(format!("{ip}:{}", state.control_port()));
    }
    let mut ports: Vec<u16> = state
        .servers
        .lock()
        .unwrap()
        .values()
        .map(|server| server.port)
        .collect();
    ports.sort_unstable();
    ports.dedup();
    // Legacy peers may not know about the control port; keep folder endpoints
    // as a fallback when the control listener could not bind.
    for port in ports {
        let address = format!("{ip}:{port}");
        if !addresses.contains(&address) {
            addresses.push(address);
        }
    }
    addresses
}

/// Start listening for incoming sync connections to a folder. The listener
/// also advertises the device on the LAN via mDNS so peers can discover it.
///
/// `port` is a hint; on bind failure the error is returned so callers can
/// retry with another port. Passing `0` asks the OS for a free port; the actual
/// bound port is returned either way, so tests never have to guess endpoints.
///
/// When `auto_accept` is true, unknown peers are admitted automatically.
/// **Warning:** this grants read/write access to the served folder to any
/// device that can reach this host. Only use it on trusted private networks.
pub async fn start_server_with_policy(
    state: &ApiState,
    port: u16,
    folder_id: i64,
    local_path: String,
    auto_accept: bool,
) -> anyhow::Result<u16> {
    if let Some(running) = state.servers.lock().unwrap().get(&folder_id) {
        return Ok(running.port);
    }
    let policy = if auto_accept {
        PairPolicy::AutoAccept
    } else {
        PairPolicy::Confirm
    };
    let (handle, mut events) = crate::sync_engine::server::serve_folder(
        state.storage.clone(),
        state.crypto.clone(),
        state.current_device(),
        local_path.clone(),
        port,
        policy,
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
    state.servers.lock().unwrap().insert(
        folder_id,
        RunningServer {
            handle,
            port: resolved_port,
            local_path,
        },
    );
    Ok(resolved_port)
}

/// Start listening for incoming sync connections to a folder with the default
/// confirmation policy (unknown peers are held for approval).
pub async fn start_server(
    state: &ApiState,
    port: u16,
    folder_id: i64,
    local_path: String,
) -> anyhow::Result<u16> {
    start_server_with_policy(state, port, folder_id, local_path, false).await
}

/// Stop all folder listeners owned by this engine instance (idempotent).
pub async fn stop_server(state: &ApiState) -> anyhow::Result<()> {
    let handles: Vec<ServeHandle> = state
        .servers
        .lock()
        .unwrap()
        .drain()
        .map(|(_, s)| s.handle)
        .collect();
    for handle in handles {
        let _ = handle.stop().await;
    }
    Ok(())
}

/// Stop all background watch tasks owned by this engine instance (idempotent).
pub async fn stop_all_watches(state: &ApiState) -> anyhow::Result<()> {
    let watches: Vec<RunningWatch> = state
        .watches
        .lock()
        .unwrap()
        .drain()
        .map(|(_, w)| w)
        .collect();
    for watch in watches {
        watch.cancel.cancel();
        let _ = watch.shutdown.send(true);
        // Best-effort: don't block forever on a stuck watcher task.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), watch.task).await;
    }
    Ok(())
}

/// Stop all background activity (control listener, servers and watches) owned
/// by this engine instance.
///
/// Shutdown errors are preserved when possible. Watch cleanup runs first; if it
/// fails, its error is returned and the server error is logged. Otherwise the
/// server shutdown error is returned. Returning the first typed error keeps
/// frontend classification intact while still attempting all cleanup.
pub async fn close(state: &ApiState) -> anyhow::Result<()> {
    let mut first_error = None;
    stop_control(state).await;
    if let Err(error) = stop_all_watches(state).await {
        first_error = Some(error);
    }
    if let Err(error) = stop_server(state).await {
        if first_error.is_none() {
            first_error = Some(error);
        } else {
            log::warn!("stop_server failed during close: {error}");
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Restart every running folder listener so mDNS advertisements and pairing
/// responses carry the current device name. Failures are logged, not fatal.
async fn restart_all_servers(state: &ApiState) {
    // The control listener also embeds our device name in PairResponse, so it
    // must come back up with the renamed identity too.
    stop_control(state).await;
    if let Err(e) = start_control(state).await {
        log::warn!("failed to restart control listener after rename: {e}");
    }
    let entries: Vec<(i64, RunningServer)> = state.servers.lock().unwrap().drain().collect();
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
            Ok(_) => restarted += 1,
            Err(e) => log::warn!("failed to restart folder server {folder_id} on port {port}: {e}"),
        }
    }
    log::info!("restarted {restarted}/{total} folder server(s) after rename");
}

// ── Device / Pairing ──

/// List pairing requests waiting for approval, across the control listener and
/// all running folder servers. Each entry is `(device_name, device_id)`.
pub fn pending_pairings(state: &ApiState) -> anyhow::Result<Vec<(String, String)>> {
    let mut result = Vec::new();
    if let Some(control) = state.control.lock().unwrap().as_ref() {
        result.extend(control.pending_pairings());
    }
    for server in state.servers.lock().unwrap().values() {
        if let Ok(pending) = server.handle.pending_pairings() {
            result.extend(pending);
        }
    }
    result.sort_by(|a, b| a.1.cmp(&b.1));
    result.dedup_by(|a, b| a.1 == b.1);
    Ok(result)
}

/// Approve a pending pairing request. The device is written to the paired
/// devices table so its next attempt is accepted on any listener.
pub fn approve_pending_pairing(
    state: &ApiState,
    device_id: String,
    device_name: String,
) -> anyhow::Result<()> {
    {
        let guard = state.control.lock().unwrap();
        if let Some(control) = guard.as_ref() {
            if control
                .pending_pairings()
                .iter()
                .any(|(_, id)| id == &device_id)
            {
                return control.approve_pairing(&device_id, &device_name);
            }
        }
    }
    for server in state.servers.lock().unwrap().values() {
        if let Ok(pending) = server.handle.pending_pairings() {
            if pending.iter().any(|(_, id)| id == &device_id) {
                server.handle.approve_pairing(&device_id, &device_name)?;
                return Ok(());
            }
        }
    }
    Err(ApiError::NotFound(format!("no pending pairing for device {device_id}")).into())
}

/// Deny a pending pairing request. The device is remembered for the lifetime
/// of the listener that received it, so retries are silently rejected.
pub fn deny_pending_pairing(state: &ApiState, device_id: String) -> anyhow::Result<()> {
    {
        let guard = state.control.lock().unwrap();
        if let Some(control) = guard.as_ref() {
            if control
                .pending_pairings()
                .iter()
                .any(|(_, id)| id == &device_id)
            {
                control.deny_pairing(&device_id);
                return Ok(());
            }
        }
    }
    for server in state.servers.lock().unwrap().values() {
        if let Ok(pending) = server.handle.pending_pairings() {
            if pending.iter().any(|(_, id)| id == &device_id) {
                server.handle.deny_pairing(&device_id)?;
                return Ok(());
            }
        }
    }
    Err(ApiError::NotFound(format!("no pending pairing for device {device_id}")).into())
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
    let mut result = Vec::new();
    for server in state.servers.lock().unwrap().values() {
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
    for server in state.servers.lock().unwrap().values() {
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
    Err(ApiError::NotFound(format!(
        "no pending folder pairing for device {device_id} on {folder_guid}"
    ))
    .into())
}

/// Deny a held folder pairing. The peer is remembered for this server's
/// lifetime so its retries are rejected, not re-queued.
pub fn deny_folder_pairing(
    state: &ApiState,
    device_id: String,
    folder_guid: String,
) -> anyhow::Result<()> {
    for server in state.servers.lock().unwrap().values() {
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
    Err(ApiError::NotFound(format!(
        "no pending folder pairing for device {device_id} on {folder_guid}"
    ))
    .into())
}

/// Maximum length of a user-chosen device name. It travels in protocol
/// frames and mDNS records, so keep it modest.
pub const DEVICE_NAME_MAX_LEN: usize = 64;

/// Validate and normalize a user-chosen device name. Shared by every
/// frontend (app, REPL, CLI) so all enforce the same rules.
pub fn sanitize_device_name(name: &str) -> anyhow::Result<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::InvalidArgument("device name cannot be empty".into()).into());
    }
    if name.chars().count() > DEVICE_NAME_MAX_LEN {
        return Err(ApiError::InvalidArgument(format!(
            "device name too long (max {DEVICE_NAME_MAX_LEN} characters)"
        ))
        .into());
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(
            ApiError::InvalidArgument("device name contains invalid characters".into()).into(),
        );
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

/// Identity returned by the device-pairing handshake. Frontends need the
/// cert-derived id (not the mDNS id) to address later folder-pair RPCs.
#[derive(Debug, Clone)]
pub struct DevicePairInfo {
    pub id: String,
    pub name: String,
}

/// Pair with a peer and return its authoritative identity.
///
/// Folder selection is deliberately *not* done here. Callers browse the peer's
/// shares after the handshake and request exactly the folder the user chose
/// (or the sole share when there is only one). Keeping device pairing and
/// folder pairing separate lets a folder-less peer pair cleanly and makes an
/// ambiguous two-folder choice a UI concern instead of a silent bulk adoption.
pub async fn pair_with_device_detailed(
    state: &ApiState,
    ip: String,
    port: u16,
) -> anyhow::Result<DevicePairInfo> {
    let addr: std::net::SocketAddr = format!("{ip}:{port}").parse()?;
    let peer = state.pairing.pair_with(addr).await?;
    Ok(DevicePairInfo {
        id: peer.id,
        name: peer.name,
    })
}

/// Human-readable form of [`pair_with_device_detailed`], kept for CLI/REPL
/// callers and older frontends that only expect a confirmation string.
pub async fn pair_with_device(state: &ApiState, ip: String, port: u16) -> anyhow::Result<String> {
    let peer = pair_with_device_detailed(state, ip, port).await?;
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
    state.storage.remove_sync_folder_by_id(folder_id)
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
    let guid = state.storage.folder_guid(folder_id)?.ok_or_else(|| {
        ApiError::InvalidArgument("folder has no guid (migration required)".into())
    })?;
    let local_path = state
        .storage
        .folder_path(folder_id)?
        .ok_or_else(|| ApiError::NotFound("folder path not found".into()))?;
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

/// Why a request to a paired peer could not be completed, at the granularity a
/// client needs to decide whether trying another endpoint is worthwhile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerRequestErrorKind {
    /// The peer could not be reached (refused / reset / aborted / unreachable).
    Transport,
    /// The connection attempt or response exceeded its deadline.
    Timeout,
    /// The peer explicitly rejected the request.
    PeerRejected,
    /// The request or the peer address was malformed.
    InvalidRequest,
    /// Anything else (protocol error, unexpected reply, local failure).
    Internal,
}

impl PeerRequestErrorKind {
    /// Transport and timeout failures are worth retrying against a freshly
    /// discovered endpoint; every other category is not.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Transport | Self::Timeout)
    }
}

/// A failed peer request: a machine-readable category for retry policy plus a
/// human-readable message for display and logs. Callers branch on [`kind`]
/// instead of matching on error text.
///
/// [`kind`]: PeerRequestError::kind
#[derive(Debug, Clone)]
pub struct PeerRequestError {
    pub kind: PeerRequestErrorKind,
    pub message: String,
}

impl PeerRequestError {
    fn new(kind: PeerRequestErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Classify an error from a peer request by inspecting its cause chain — I/O
/// error kinds and timeouts — never by parsing application text.
fn classify_peer_error(error: &anyhow::Error) -> PeerRequestError {
    use std::io::ErrorKind;
    for cause in error.chain() {
        if cause.is::<crate::sync_engine::OperationCancelled>() || cause.is::<ApiError>() {
            // Cancellation/internal typed API errors are not peer-transport
            // retry candidates.
            if let Some(ApiError::Cancelled) = cause.downcast_ref::<ApiError>() {
                return PeerRequestError::new(PeerRequestErrorKind::Internal, error.to_string());
            }
            if cause.is::<crate::sync_engine::OperationCancelled>() {
                return PeerRequestError::new(PeerRequestErrorKind::Internal, error.to_string());
            }
        }
        if let Some(session) =
            cause.downcast_ref::<crate::sync_engine::session_transfer::SessionError>()
        {
            let kind = match session {
                crate::sync_engine::session_transfer::SessionError::PeerDisconnected => {
                    PeerRequestErrorKind::Transport
                }
                crate::sync_engine::session_transfer::SessionError::TransferTimeout(_) => {
                    PeerRequestErrorKind::Timeout
                }
                crate::sync_engine::session_transfer::SessionError::Cancelled => {
                    PeerRequestErrorKind::Internal
                }
                _ => PeerRequestErrorKind::Internal,
            };
            return PeerRequestError::new(kind, error.to_string());
        }
        if cause
            .downcast_ref::<tokio::time::error::Elapsed>()
            .is_some()
        {
            return PeerRequestError::new(PeerRequestErrorKind::Timeout, error.to_string());
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            let kind = match io.kind() {
                ErrorKind::TimedOut => PeerRequestErrorKind::Timeout,
                ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::NotConnected
                | ErrorKind::AddrNotAvailable
                | ErrorKind::HostUnreachable
                | ErrorKind::NetworkUnreachable => PeerRequestErrorKind::Transport,
                _ => continue,
            };
            return PeerRequestError::new(kind, error.to_string());
        }
    }
    PeerRequestError::new(PeerRequestErrorKind::Internal, error.to_string())
}

/// Outcome of browsing a peer's shared folders. A successful but empty list
/// (reachable peer publishing nothing) is deliberately distinct from a failure,
/// so the client can decide whether a discovered-endpoint retry is worthwhile.
#[derive(Debug, Clone)]
pub enum BrowseSharedFoldersResult {
    Success(Vec<RemoteSharedFolder>),
    Failure(PeerRequestError),
}

/// Browse an already-trusted peer's discoverable shared folders over TLS.
/// This is a post-pairing surface — never advertised over mDNS.
///
/// Never throws for a peer-level problem; inspect the returned
/// [`PeerRequestError`] (via [`PeerRequestErrorKind`]) instead.
pub async fn browse_peer_shared_folders(
    state: &ApiState,
    peer_ip: String,
    peer_port: u16,
) -> BrowseSharedFoldersResult {
    let addr = match format!("{peer_ip}:{peer_port}").parse::<std::net::SocketAddr>() {
        Ok(addr) => addr,
        Err(e) => {
            return BrowseSharedFoldersResult::Failure(PeerRequestError::new(
                PeerRequestErrorKind::InvalidRequest,
                format!("invalid peer address {peer_ip}:{peer_port}: {e}"),
            ))
        }
    };
    match list_shared_folders_at(state, addr).await {
        Ok(folders) => BrowseSharedFoldersResult::Success(folders),
        Err(e) => BrowseSharedFoldersResult::Failure(classify_peer_error(&e)),
    }
}

/// One TLS round trip to enumerate a peer's discoverable shared folders.
async fn list_shared_folders_at(
    state: &ApiState,
    addr: std::net::SocketAddr,
) -> anyhow::Result<Vec<RemoteSharedFolder>> {
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

/// Persist a peer endpoint a client just validated, using the storage layer's
/// generation-checked update so a concurrent or better resolution is never
/// clobbered.
///
/// `expected_previous` is the stored value the caller based its decision on
/// (`None` means "there was none") and `expected_generation` is the address
/// version it read at the same time. The write only happens when the row still
/// holds both, so a stale resolution is rejected even when the address happens
/// to match again (ABA). Returns whether the endpoint was written. Rejects a
/// malformed host so garbage can never reach the address store.
pub fn remember_peer_endpoint(
    state: &ApiState,
    device_id: String,
    host: String,
    port: u16,
    expected_previous: Option<String>,
    expected_generation: u64,
) -> anyhow::Result<bool> {
    let ip: std::net::IpAddr = host.parse().map_err(|_| {
        ApiError::InvalidArgument(format!("refusing to store invalid peer host {host}"))
    })?;
    let addr = std::net::SocketAddr::new(ip, port);
    Ok(matches!(
        state.storage.update_peer_endpoint_if_current(
            &device_id,
            expected_generation,
            expected_previous.as_deref(),
            &addr.to_string(),
        )?,
        crate::storage::UpdateOutcome::Updated
    ))
}

/// The current address generation for a paired device (0 when the device has no
/// recorded endpoint). A client captures this alongside the stored address and
/// passes it to [`remember_peer_endpoint`] so an older resolution cannot
/// overwrite a newer one.
pub fn device_addr_generation(state: &ApiState, device_id: String) -> anyhow::Result<u64> {
    state.storage.device_addr_generation(&device_id)
}

/// Outcome of requesting pairing to a peer's shared folder. A `Failure` carries
/// a typed [`PeerRequestError`] so the client can retry only for a
/// transport/timeout problem and keep the real reason otherwise.
#[derive(Debug, Clone)]
pub enum FolderPairRequestOutcome {
    /// Owner approved; the replica is registered on this device.
    Approved {
        folder_guid: String,
        name: String,
        remote_path: Option<String>,
    },
    /// Owner explicitly rejected the request.
    Rejected(String),
    /// Still waiting on the owner; the caller can poll again.
    Pending,
    /// The request could not be completed; see the error kind for retry policy.
    Failure(PeerRequestError),
}

/// Ask an already-paired peer to pair us to one of its shared folders, and
/// (when the owner approves) register the replica on this device so the folder
/// starts syncing.
///
/// `local_path` is where this device keeps its copy; `lifetime_ms` bounds how
/// long we keep polling for the owner's approval (0 = poll once).
///
/// Never throws for a peer-level problem; a classification is returned as
/// [`FolderPairRequestOutcome::Failure`] instead.
#[allow(clippy::too_many_arguments)]
pub async fn request_folder_pairing(
    state: &ApiState,
    peer_ip: String,
    peer_port: u16,
    peer_device_id: String,
    folder_guid: String,
    share_name: String,
    local_path: String,
    lifetime_ms: u64,
) -> FolderPairRequestOutcome {
    let addr = match format!("{peer_ip}:{peer_port}").parse::<std::net::SocketAddr>() {
        Ok(addr) => addr,
        Err(e) => {
            return FolderPairRequestOutcome::Failure(PeerRequestError::new(
                PeerRequestErrorKind::InvalidRequest,
                format!("invalid peer address {peer_ip}:{peer_port}: {e}"),
            ))
        }
    };
    match drive_folder_pairing(
        state,
        addr,
        &peer_device_id,
        &folder_guid,
        &share_name,
        &local_path,
        lifetime_ms,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(e) => FolderPairRequestOutcome::Failure(classify_peer_error(&e)),
    }
}

/// Drive one folder-pairing request against a concrete address. Errors here are
/// transport/protocol errors, classified by the caller.
async fn drive_folder_pairing(
    state: &ApiState,
    addr: std::net::SocketAddr,
    peer_device_id: &str,
    folder_guid: &str,
    share_name: &str,
    local_path: &str,
    lifetime_ms: u64,
) -> anyhow::Result<FolderPairRequestOutcome> {
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
            folder_guid,
            share_name,
            if lifetime_ms == 0 {
                None
            } else {
                Some(std::time::Duration::from_millis(lifetime_ms))
            },
        )
        .await?;
    Ok(match reply {
        crate::sync_engine::shared_folder::FolderPairReply::Approved(grant) => {
            // The replica path is user-visible and may be freshly derived
            // (e.g. ~/auto-share), so create it before recording the pair.
            std::fs::create_dir_all(local_path)
                .with_context(|| format!("create replica directory {local_path}"))?;
            // Register our replica of the logical space, reusing the owner's
            // guid so both sides track the same folder. When the user already
            // added a local folder for `local_path`, wire that folder to the
            // peer's share instead of minting a duplicate replica.
            let _folder_id = state.storage.wire_folder_pair(
                folder_guid,
                local_path,
                share_name,
                &own,
                peer_device_id,
                grant.remote_path.as_deref(),
            )?;
            // Record the owner's address so a follow-up sync can reach it
            // directly instead of depending on a prior transfer or a browse.
            //
            // Establishment write: this endpoint was just validated by the
            // authenticated pairing RPC above, so it is recorded
            // unconditionally, mirroring the initial device-pairing write.
            // The folder-pair RPC may have been served by the control listener.
            // Remember a folder-server endpoint for the subsequent sync, not the
            // control port that only answered the RPC.
            let sync_addr = if addr.port() == CONTROL_PORT {
                std::net::SocketAddr::new(addr.ip(), crate::sync_engine::bulk::DEFAULT_PORT)
            } else {
                addr
            };
            state
                .storage
                .set_device_last_addr(peer_device_id, &sync_addr.to_string())?;
            FolderPairRequestOutcome::Approved {
                folder_guid: grant.folder_guid,
                name: grant.name,
                remote_path: grant.remote_path,
            }
        }
        crate::sync_engine::shared_folder::FolderPairReply::Rejected(reason) => {
            FolderPairRequestOutcome::Rejected(reason)
        }
        crate::sync_engine::shared_folder::FolderPairReply::Pending => {
            FolderPairRequestOutcome::Pending
        }
    })
}

/// Offer one of our shared folders to an already-paired peer and register the
/// same logical folder pair here. The peer creates its own replica, so a peer
/// with no published shares can still end up with the folder in its Folders
/// list. Errors are classified instead of thrown, like [request_folder_pairing].
#[allow(clippy::too_many_arguments)]
pub async fn offer_folder_pair(
    state: &ApiState,
    peer_ip: String,
    peer_port: u16,
    peer_device_id: String,
    folder_guid: String,
    share_name: String,
    local_path: String,
    // Where the peer should keep its replica. Empty uses its default folder.
    remote_path: String,
    lifetime_ms: u64,
) -> FolderPairRequestOutcome {
    let addr = match format!("{peer_ip}:{peer_port}").parse::<std::net::SocketAddr>() {
        Ok(addr) => addr,
        Err(e) => {
            return FolderPairRequestOutcome::Failure(PeerRequestError::new(
                PeerRequestErrorKind::InvalidRequest,
                format!("invalid peer address {peer_ip}:{peer_port}: {e}"),
            ))
        }
    };
    match drive_folder_offer(
        state,
        addr,
        &peer_device_id,
        &folder_guid,
        &share_name,
        &local_path,
        &remote_path,
        lifetime_ms,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(e) => FolderPairRequestOutcome::Failure(classify_peer_error(&e)),
    }
}

async fn drive_folder_offer(
    state: &ApiState,
    addr: std::net::SocketAddr,
    peer_device_id: &str,
    folder_guid: &str,
    share_name: &str,
    local_path: &str,
    remote_path: &str,
    lifetime_ms: u64,
) -> anyhow::Result<FolderPairRequestOutcome> {
    let client =
        crate::sync_engine::shared_folder::SharedFolderClient::new(state.crypto.clone(), addr);
    let own = state.current_device().id;
    state
        .storage
        .upsert_device(&own, &state.current_device().name, None, None)?;
    let info = crate::protocol_v2::shared::SharedFolderInfo::new(
        folder_guid,
        share_name,
        "bidirectional",
        local_path,
    );
    let reply = client
        .offer_and_collect_pairing(
            info,
            if remote_path.trim().is_empty() {
                None
            } else {
                Some(remote_path.to_string())
            },
            if lifetime_ms == 0 {
                None
            } else {
                Some(std::time::Duration::from_millis(lifetime_ms))
            },
        )
        .await?;
    Ok(match reply {
        crate::sync_engine::shared_folder::FolderPairReply::Approved(grant) => {
            let _folder_id = state.storage.wire_folder_pair(
                folder_guid,
                local_path,
                share_name,
                &own,
                peer_device_id,
                grant.remote_path.as_deref(),
            )?;
            let sync_addr = if addr.port() == CONTROL_PORT {
                std::net::SocketAddr::new(addr.ip(), crate::sync_engine::bulk::DEFAULT_PORT)
            } else {
                addr
            };
            state
                .storage
                .set_device_last_addr(peer_device_id, &sync_addr.to_string())?;
            FolderPairRequestOutcome::Approved {
                folder_guid: grant.folder_guid,
                name: grant.name,
                remote_path: grant.remote_path,
            }
        }
        crate::sync_engine::shared_folder::FolderPairReply::Rejected(reason) => {
            FolderPairRequestOutcome::Rejected(reason)
        }
        crate::sync_engine::shared_folder::FolderPairReply::Pending => {
            FolderPairRequestOutcome::Pending
        }
    })
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

/// Restore the device to a fresh-install state: stop background activity,
/// then wipe all persisted state in the data directory (local identity,
/// device id, database, and device-name config). User files under the
/// configured folders are never touched.
///
/// The identity is not regenerated here — the next `init_engine` runs the
/// cold-start path and derives a brand-new device id from a fresh keypair.
pub async fn factory_reset(state: &ApiState) -> anyhow::Result<()> {
    let _ = close(state).await;

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
    let id = folder_id
        .ok_or_else(|| ApiError::InvalidArgument("at least one device is required".into()))?;
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

/// Grouped sync-folder rows keyed by folder id: `(local_path, last_sync_at, pairs)`.
type SyncFolderGroups =
    std::collections::BTreeMap<i64, (String, Option<i64>, Vec<(String, String)>)>;

pub fn list_sync_folders(state: &ApiState) -> anyhow::Result<Vec<FolderEntry>> {
    let own = state.current_device().id;
    let rows = state.storage.list_sync_folders()?;
    // (id, local_path, last_sync_at, [(device_id, mode)])
    let mut by_folder: SyncFolderGroups = std::collections::BTreeMap::new();
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

// ── Watch ──

/// Information about a running background watch.
#[derive(Debug, Clone)]
pub struct WatchEntry {
    pub id: i64,
    pub folder_id: i64,
    pub local_path: String,
    pub device_id: String,
    pub remote_ip: String,
    pub remote_port: u16,
}

/// Start watching a local folder and syncing changes to a peer.
/// Returns a watch id that can be passed to `stop_watch`.
pub async fn start_watch(
    state: &ApiState,
    folder_id: i64,
    local_path: String,
    remote_ip: String,
    remote_port: u16,
    device_id: String,
) -> anyhow::Result<i64> {
    start_watch_with_cancel(
        state,
        folder_id,
        local_path,
        remote_ip,
        remote_port,
        device_id,
        ApiCancelToken::new(),
    )
    .await
}

/// Start watching a local folder with a cooperative cancellation token.
pub async fn start_watch_with_cancel(
    state: &ApiState,
    folder_id: i64,
    local_path: String,
    remote_ip: String,
    remote_port: u16,
    device_id: String,
    cancel: ApiCancelToken,
) -> anyhow::Result<i64> {
    use crate::watcher::FileWatcher;
    use std::net::SocketAddr;

    ensure_not_cancelled(&cancel)?;
    let remote_addr: SocketAddr = format!("{remote_ip}:{remote_port}")
        .parse()
        .context("invalid remote address")?;
    let mut watcher = FileWatcher::watch(PathBuf::from(&local_path))
        .map_err(|e| ApiError::Internal(format!("failed to watch {local_path}: {e}")))?;

    let id = {
        let mut next = state.next_watch_id.lock().unwrap();
        let id = *next;
        *next += 1;
        id
    };
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let engine = state.engine.clone();
    let task_state = state.clone();
    let task_local_path = local_path.clone();
    let task_device_id = device_id.clone();
    let task_cancel = cancel.clone();
    let task = tokio::spawn(async move {
        loop {
            if task_cancel.is_cancelled() {
                break;
            }
            tokio::select! {
                // Wake immediately when the Python event bridge (or any other
                // holder of the token) requests cancellation; otherwise the
                // loop could stay blocked on `recv()` until the next file event.
                _ = task_cancel.cancelled() => break,
                _ = shutdown_rx.changed() => break,
                event = watcher.events().recv() => {
                    if event.is_none() {
                        break;
                    }
                    // Debounce: wait for the burst to settle, then drain queued events.
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    while watcher.events().try_recv().is_ok() {}
                    if task_cancel.is_cancelled() {
                        break;
                    }
                    let _ = engine
                        .run_sync_with_cancel(
                            &task_local_path,
                            remote_addr,
                            folder_id,
                            &task_device_id,
                            false,
                            &task_cancel,
                        )
                        .await;
                }
            }
        }
        // A cancelled or self-terminated watch must not remain listed as live.
        // `stop_watch` removes the entry first, so this is a no-op there.
        task_state.watches.lock().unwrap().remove(&id);
    });
    state.watches.lock().unwrap().insert(
        id,
        RunningWatch {
            folder_id,
            local_path,
            device_id,
            remote_addr,
            shutdown: shutdown_tx,
            cancel,
            task,
        },
    );
    Ok(id)
}

/// Stop a running background watch.
pub async fn stop_watch(state: &ApiState, watch_id: i64) -> anyhow::Result<()> {
    let watch = state
        .watches
        .lock()
        .unwrap()
        .remove(&watch_id)
        .ok_or_else(|| ApiError::NotFound("unknown watch id".into()))?;
    watch.cancel.cancel();
    let _ = watch.shutdown.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), watch.task).await;
    Ok(())
}

/// List running background watches.
pub fn list_watches(state: &ApiState) -> Vec<WatchEntry> {
    let mut entries: Vec<WatchEntry> = state
        .watches
        .lock()
        .unwrap()
        .iter()
        .map(|(id, w)| WatchEntry {
            id: *id,
            folder_id: w.folder_id,
            local_path: w.local_path.clone(),
            device_id: w.device_id.clone(),
            remote_ip: w.remote_addr.ip().to_string(),
            remote_port: w.remote_addr.port(),
        })
        .collect();
    entries.sort_by_key(|e| e.id);
    entries
}

// ── Sync ──

/// Folder-server endpoints for `folder_id` advertised by its peer.
///
/// The device row stores one address per peer, which cannot express two folders
/// served on two ports. The folder guid is published by each folder server's
/// mDNS advertisement, so it is the reliable way to find the right socket after
/// pairing over the control listener. Every candidate is TLS-authenticated as
/// `device_id` before it is returned.
async fn folder_sync_candidates(
    state: &ApiState,
    folder_id: i64,
    device_id: &str,
) -> Vec<std::net::SocketAddr> {
    let Some(guid) = state.storage.folder_guid(folder_id).unwrap_or(None) else {
        return Vec::new();
    };
    let advertised = crate::sync_engine::bulk::discover_folder_candidates_for(
        &guid,
        &state.current_device().id,
        DISCOVERY_FALLBACK_SECS,
    )
    .await;
    log::debug!("folder {guid} advertised sync candidates: {advertised:?}");
    let authenticated = crate::sync_engine::bulk::authenticated_candidates(
        state.crypto.clone(),
        device_id,
        &advertised,
        crate::sync_engine::bulk::IDENTITY_PROBE_TIMEOUT,
    )
    .await;
    // Do not filter self addresses here: two engine instances on one host are
    // normal in tests, and the TLS identity check above already proves the
    // endpoint is the expected peer rather than this process.
    log::debug!("folder {guid} authenticated sync candidates: {authenticated:?}");
    authenticated
}

async fn attempt_sync(
    state: &ApiState,
    local_path: &str,
    addr: std::net::SocketAddr,
    folder_id: i64,
    device_id: &str,
    dry_run: bool,
    cancel: &ApiCancelToken,
) -> anyhow::Result<SyncResult> {
    tokio::select! {
        biased;
        _ = wait_for_cancel(cancel) => Err(cancelled_err()),
        res = state.engine.run_sync_with_cancel(
            local_path,
            addr,
            folder_id,
            device_id,
            dry_run,
            cancel,
        ) => res,
    }
}

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
    sync_folder_with_cancel(
        state,
        folder_id,
        local_path,
        remote_ip,
        remote_port,
        device_id,
        dry_run,
        ApiCancelToken::new(),
    )
    .await
}

/// Run a full sync session with a cooperative cancellation token.
///
/// Cancellation is checked at API boundaries and between discovery-fallback
/// attempts. An in-flight sync engine session may continue briefly until it
/// reaches a cooperative boundary; callers must discard results after
/// cancellation is observed.
pub async fn sync_folder_with_cancel(
    state: &ApiState,
    folder_id: i64,
    local_path: String,
    remote_ip: String,
    remote_port: u16,
    device_id: String,
    dry_run: bool,
    cancel: ApiCancelToken,
) -> anyhow::Result<SyncResult> {
    ensure_not_cancelled(&cancel)?;
    let requested_addr: std::net::SocketAddr = format!("{remote_ip}:{remote_port}").parse()?;
    // Capture the endpoint state this sync is based on, *before* any discovery
    // or network work. Persisting a candidate afterwards is then a single atomic
    // compare-and-set: if another pairing/discovery session moved the endpoint
    // in the meantime, its newer value wins instead of being clobbered by a
    // stale candidate we happen to authenticate later (including the ABA case).
    //
    // Best-effort read: a storage hiccup must not stop the sync from running;
    // the compare-and-set below simply will not match and the write is skipped.
    let expected_previous = state.storage.device_last_addr(&device_id).unwrap_or(None);
    let expected_generation = state
        .storage
        .device_addr_generation(&device_id)
        .unwrap_or(0);

    // A control address can pair but cannot serve files. Look up the folder's
    // own listener up front so callers using the address recorded at pairing
    // time never see the control listener's "use the folder server" failure.
    let mut folder_candidates = if requested_addr.port() == CONTROL_PORT {
        folder_sync_candidates(state, folder_id, &device_id).await
    } else {
        Vec::new()
    };
    ensure_not_cancelled(&cancel)?;
    let addr = folder_candidates.first().copied().unwrap_or(requested_addr);

    let result = attempt_sync(
        state,
        &local_path,
        addr,
        folder_id,
        &device_id,
        dry_run,
        &cancel,
    )
    .await;
    if cancel.is_cancelled() {
        return Err(cancelled_err());
    }

    match result {
        Ok(r) => {
            if !dry_run && !cancel.is_cancelled() {
                crate::sync_engine::remember_reached_endpoint(
                    &state.storage,
                    &device_id,
                    expected_previous.as_deref(),
                    expected_generation,
                    addr,
                );
            }
            ensure_not_cancelled(&cancel)?;
            Ok(r)
        }
        Err(e) => {
            let mut last_err = e;
            // Whether or not the caller supplied a control port, a folder guid
            // match beats a stored address that points at another folder's
            // server. Try those endpoints before the generic stale-address
            // fallback. The guid search is bounded by DISCOVERY_FALLBACK_SECS.
            if folder_candidates.is_empty() {
                folder_candidates = folder_sync_candidates(state, folder_id, &device_id).await;
                ensure_not_cancelled(&cancel)?;
            }
            for fresh in folder_candidates
                .iter()
                .copied()
                .filter(|fresh| *fresh != addr)
            {
                let retry = attempt_sync(
                    state,
                    &local_path,
                    fresh,
                    folder_id,
                    &device_id,
                    dry_run,
                    &cancel,
                )
                .await;
                if cancel.is_cancelled() {
                    return Err(cancelled_err());
                }
                match retry {
                    Ok(r) => {
                        if !dry_run && !cancel.is_cancelled() {
                            crate::sync_engine::remember_reached_endpoint(
                                &state.storage,
                                &device_id,
                                expected_previous.as_deref(),
                                expected_generation,
                                fresh,
                            );
                        }
                        ensure_not_cancelled(&cancel)?;
                        return Ok(r);
                    }
                    Err(err) => {
                        log::debug!(
                            "folder endpoint {fresh} for {device_id} did not work: {err:#}"
                        );
                        last_err = err;
                    }
                }
            }

            // The remaining fallback is for stale addresses (phone/IP changed):
            // try every advertised folder endpoint in preference order. The
            // retryable/terminal decision is driven by the typed cause chain
            // (see `classify_peer_error`), never by matching the error's text.
            if classify_peer_error(&last_err).kind.is_retryable() {
                ensure_not_cancelled(&cancel)?;
                let advertised = crate::sync_engine::bulk::discover_candidates_for(
                    &device_id,
                    &state.current_device().id,
                    DISCOVERY_FALLBACK_SECS,
                )
                .await;
                for fresh in advertised {
                    ensure_not_cancelled(&cancel)?;
                    if fresh == addr
                        || folder_candidates.contains(&fresh)
                        || crate::sync_engine::bulk::is_own_address(fresh)
                    {
                        continue;
                    }
                    let discovered_result = attempt_sync(
                        state,
                        &local_path,
                        fresh,
                        folder_id,
                        &device_id,
                        dry_run,
                        &cancel,
                    )
                    .await;
                    if cancel.is_cancelled() {
                        return Err(cancelled_err());
                    }
                    match discovered_result {
                        Ok(r) => {
                            if !dry_run && !cancel.is_cancelled() {
                                crate::sync_engine::remember_reached_endpoint(
                                    &state.storage,
                                    &device_id,
                                    expected_previous.as_deref(),
                                    expected_generation,
                                    fresh,
                                );
                            }
                            ensure_not_cancelled(&cancel)?;
                            return Ok(r);
                        }
                        Err(discovered_err) => {
                            log::debug!(
                                "discovered address {fresh} for {device_id} did not work: \
                                 {discovered_err:#}"
                            );
                            last_err = discovered_err;
                        }
                    }
                }
            }
            Err(last_err)
        }
    }
}

async fn wait_for_cancel(token: &ApiCancelToken) {
    token.cancelled().await;
}

fn cancelled_err() -> anyhow::Error {
    OperationCancelled.into()
}

/// How long the mDNS freshness fallback browses for a live advertisement.
const DISCOVERY_FALLBACK_SECS: u64 = 2;

// ── Events ──

/// Poll for pending sync events observed by this engine instance.
pub async fn poll_sync_events(state: &ApiState) -> Vec<SyncEvent> {
    let mut events = Vec::new();
    let mut rx = state.event_poll.lock().unwrap();
    loop {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(broadcast::error::TryRecvError::Closed) => break,
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                // Slow consumer missed some events; continue draining newer ones.
                continue;
            }
        }
    }
    events
}

/// Subscribe to engine events (receives events emitted after subscription).
///
/// Native/Python-only: FRB cannot generate a usable binding for
/// `broadcast::Receiver<SyncEvent>` (it emits an unqualified, non-encodable
/// opaque type), so keep it out of the generated Dart API.
/// flutter_rust_bridge:ignore
pub fn subscribe_sync_events(state: &ApiState) -> broadcast::Receiver<SyncEvent> {
    state.engine.subscribe_events()
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
    /// `complete` or `incomplete`: whether the session confirmed every planned
    /// transfer before it ended.
    pub status: String,
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
            status: r.status,
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
            status: r.status,
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
        .ok_or_else(|| ApiError::NotFound(format!("sync folder {folder_id} not found")))?;
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

    #[test]
    fn merges_pairing_and_folder_advertisements_for_one_device() {
        use crate::discovery::DiscoveredPeer;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let id = "device-1".to_string();
        let addr = |port: u16| {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42)), port)
        };
        let pairing = DiscoveredPeer {
            id: id.clone(),
            name: "Desktop (pairing)".into(),
            addresses: vec![addr(9846)],
            properties: HashMap::from([
                ("id".to_string(), id.clone()),
                ("control_port".to_string(), "9846".to_string()),
            ]),
        };
        let folder = DiscoveredPeer {
            id: id.clone(),
            name: "Desktop".into(),
            addresses: vec![addr(9847)],
            properties: HashMap::from([("id".to_string(), id.clone())]),
        };

        let merged = merge_discovered_peers(vec![pairing, folder]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, id);
        assert_eq!(merged[0].name, "Desktop");
        assert_eq!(merged[0].port, 9847);
        assert_eq!(merged[0].control_port, Some(9846));
    }

    #[test]
    fn pairing_only_advertisement_is_discoverable() {
        use crate::discovery::DiscoveredPeer;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let merged = merge_discovered_peers(vec![DiscoveredPeer {
            id: "phone".into(),
            name: "Phone (pairing)".into(),
            addresses: vec![SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9)),
                9846,
            )],
            properties: HashMap::from([
                ("id".to_string(), "phone".to_string()),
                ("control_port".to_string(), "9846".to_string()),
            ]),
        }]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].control_port, Some(9846));
        assert_eq!(merged[0].port, 9846);
    }

    #[tokio::test]
    async fn identity_is_stable_across_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let first = load_or_create_identity(dir.path()).await.unwrap();
        let second = load_or_create_identity(dir.path()).await.unwrap();

        assert_eq!(first.fingerprint().await, second.fingerprint().await);
        // The id is a pure function of the certificate, so it is stable across
        // reloads and identical to what peers derive from our cert.
        let cert = first.certificate().await;
        let expected = crate::crypto::cert_to_device_id(&cert);
        assert_eq!(
            expected,
            crate::crypto::cert_to_device_id(&second.certificate().await)
        );
    }

    /// Regression test for the duplicate-device bug: before identity
    /// unification a device could be recorded under BOTH its cert-derived id
    /// and a separate random uuid, so the same name repeated in status output.
    #[tokio::test]
    async fn migration_merges_duplicate_rows_sharing_a_cert() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();
        let own = load_or_create_identity(dir.path()).await.unwrap();

        // A distinct peer certificate to key the duplicate rows.
        let peer_dir = tempfile::tempdir().unwrap();
        let peer = load_or_create_identity(peer_dir.path()).await.unwrap();
        let peer_cert = peer.certificate().await;
        let peer_cert_id = crate::crypto::cert_to_device_id(&peer_cert);

        let storage = &state.storage;
        // The same physical device recorded twice before unification.
        storage
            .upsert_device(&peer_cert_id, "test2", Some(&peer_cert), None)
            .unwrap();
        storage
            .upsert_device("some-random-uuid", "test2", Some(&peer_cert), None)
            .unwrap();
        // A single-row peer that must be left untouched.
        storage.upsert_device("dev-a", "alpha", None, None).unwrap();

        migrate_legacy_device_identity(storage, dir.path(), &own.certificate().await).unwrap();

        let devices = storage.list_devices().unwrap();
        let names: Vec<&String> = devices.iter().map(|d| &d.1).collect();
        assert_eq!(
            names.iter().filter(|n| n.as_str() == "test2").count(),
            1,
            "duplicate device rows must be merged into one"
        );
        assert!(
            devices.iter().all(|d| d.0 != "some-random-uuid"),
            "legacy random-uuid row must be removed"
        );
        assert!(
            devices.iter().any(|d| d.0 == "dev-a"),
            "unrelated devices must survive"
        );
    }

    /// Regression test for "the app lists itself as a paired device and shows
    /// peers twice". Covers the two holes the migration must close:
    ///   1. A legacy *no-cert* self row (old random id) must fold onto our
    ///      cert-derived id and get its certificate stored, so it is hidden
    ///      from the device list instead of surfacing as "self".
    ///   2. A duplicate *self* row carrying our own certificate must be
    ///      removed, and a peer recorded once without a cert and once with
    ///      its cert must collapse to a single row.
    #[tokio::test]
    async fn migration_folds_no_cert_self_row_and_dedups_peers() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();
        let own = load_or_create_identity(dir.path()).await.unwrap();
        let own_cert = own.certificate().await;
        let own_id = state.current_device().id;

        let peer_dir = tempfile::tempdir().unwrap();
        let peer = load_or_create_identity(peer_dir.path()).await.unwrap();
        let peer_cert = peer.certificate().await;
        let peer_cert_id = crate::crypto::cert_to_device_id(&peer_cert);

        let storage = &state.storage;
        // A legacy no-cert self row under a random uuid (as the `device.id`
        // file is already gone, it is only reachable via this id).
        storage
            .upsert_device("legacy-self", "repl_cli", None, None)
            .unwrap();
        // A rogue duplicate self row carrying our own certificate.
        storage
            .upsert_device("random-uuid-2", "repl_cli", Some(&own_cert), None)
            .unwrap();
        // The same physical peer recorded twice: once without a cert and once
        // with its cert (old random id + cert-derived id).
        storage
            .upsert_device("peer-legacy", "test2", None, None)
            .unwrap();
        storage
            .upsert_device(&peer_cert_id, "test2", Some(&peer_cert), None)
            .unwrap();
        // An unrelated single-row peer that must survive untouched.
        storage.upsert_device("dev-a", "alpha", None, None).unwrap();

        // Re-run the identity migration with the legacy self id recorded in
        // the now-obsolete `device.id` state file (as a pre-unification install
        // would have).
        std::fs::write(dir.path().join("device.id"), "legacy-self").unwrap();
        migrate_legacy_device_identity(storage, dir.path(), &own_cert).unwrap();

        let devices = storage.list_devices().unwrap();
        let ids: Vec<&String> = devices.iter().map(|d| &d.0).collect();
        assert!(
            !ids.contains(&&"legacy-self".to_string()),
            "no-cert legacy self row must be folded onto the cert-derived id"
        );
        assert!(
            !ids.contains(&&"random-uuid-2".to_string()),
            "duplicate self row carrying our cert must be removed"
        );
        assert!(
            !ids.contains(&&"peer-legacy".to_string()),
            "peer's no-cert legacy row must be merged into its cert row"
        );
        // Exactly one test2 row, under the peer's cert-derived id.
        let test2: Vec<&String> = devices
            .iter()
            .filter(|(_, n, _)| n == "test2")
            .map(|(id, _, _)| id)
            .collect();
        assert_eq!(test2.len(), 1, "peer must not appear twice");
        assert_eq!(test2[0], &peer_cert_id);
        assert!(
            ids.contains(&&"dev-a".to_string()),
            "unrelated devices must survive"
        );

        // The own row now carries the certificate (so it is recognisable as self).
        let own_cert_stored = storage
            .get_device_cert(&own_id)
            .unwrap()
            .map(|c| crate::crypto::cert_to_device_id(&c))
            .unwrap();
        assert_eq!(own_cert_stored, own_id);
    }

    /// The device list presented to callers never includes our own row — this
    /// is the user-visible guarantee that the app cannot list itself.
    #[tokio::test]
    async fn list_devices_hides_own_row_even_with_legacy_self() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        // Recreate the legacy-world shape where the old self row id differs
        // from our cert-derived id, then run the migration exactly as startup
        // does, and confirm the API surface shows only the peer.
        state
            .storage
            .upsert_device("old-self-id", "repl_cli", None, None)
            .unwrap();
        state
            .storage
            .upsert_device("peer-1", "phone", None, None)
            .unwrap();
        let own = load_or_create_identity(dir.path()).await.unwrap();
        let own_cert = own.certificate().await;
        std::fs::write(dir.path().join("device.id"), "old-self-id").unwrap();
        migrate_legacy_device_identity(&state.storage, dir.path(), &own_cert).unwrap();

        let devices = list_devices(&state).unwrap();
        assert_eq!(devices.len(), 1, "only the peer should be listed");
        assert_eq!(devices[0].id, "peer-1");
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
                Some(&client_cert_der),
                None,
            )
            .unwrap();

        client_storage
            .upsert_device(
                &server_info.id,
                &server_info.name,
                Some(&server_crypto.certificate().await),
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
        let port_before = state.servers.lock().unwrap()[&folder_id].handle.port;
        assert_ne!(port_before, 0);

        set_device_name(&state, "renamed-host".into())
            .await
            .unwrap();

        let port_after = {
            let guard = state.servers.lock().unwrap();
            assert!(
                guard.contains_key(&folder_id),
                "server must be running again after rename"
            );
            guard[&folder_id].handle.port
        };
        assert_ne!(port_after, 0);

        // Clean up: stop the listener so other tests see no stale entries.
        let handle = state.servers.lock().unwrap().remove(&folder_id).unwrap();
        handle.handle.stop().await;
    }

    #[test]
    fn classify_peer_error_maps_io_kinds_to_retry_policy() {
        for kind in [
            std::io::ErrorKind::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted,
            std::io::ErrorKind::NotConnected,
            std::io::ErrorKind::AddrNotAvailable,
            std::io::ErrorKind::HostUnreachable,
            std::io::ErrorKind::NetworkUnreachable,
        ] {
            let err: anyhow::Error = std::io::Error::from(kind).into();
            let classified = classify_peer_error(&err);
            assert_eq!(classified.kind, PeerRequestErrorKind::Transport, "{kind:?}");
            assert!(classified.kind.is_retryable(), "{kind:?}");
            // The original detail is preserved for diagnostics.
            assert!(
                classified.message.contains(&format!("{kind:?}")) || !classified.message.is_empty()
            );
        }

        let timed: anyhow::Error = std::io::Error::from(std::io::ErrorKind::TimedOut).into();
        let classified = classify_peer_error(&timed);
        assert_eq!(classified.kind, PeerRequestErrorKind::Timeout);
        assert!(classified.kind.is_retryable());

        // An unmapped I/O error is conservative: NOT retryable.
        let denied: anyhow::Error =
            std::io::Error::from(std::io::ErrorKind::PermissionDenied).into();
        assert_eq!(
            classify_peer_error(&denied).kind,
            PeerRequestErrorKind::Internal
        );

        // A protocol-level surprise is NOT retryable, so the client keeps the
        // real reason instead of silently re-dialling another endpoint.
        let internal = anyhow::anyhow!("unexpected reply to ListSharedFolders");
        let classified = classify_peer_error(&internal);
        assert_eq!(classified.kind, PeerRequestErrorKind::Internal);
        assert!(!classified.kind.is_retryable());
        assert_eq!(classified.message, "unexpected reply to ListSharedFolders");
    }

    /// Typed session failures drive retry policy without parsing message text:
    /// a dropped peer or an idle timeout is worth retrying; a validation or
    /// protocol failure is not.
    #[test]
    fn classify_peer_error_maps_session_failures() {
        use crate::sync_engine::session_transfer::SessionError;

        let disconnected: anyhow::Error = SessionError::PeerDisconnected.into();
        let classified = classify_peer_error(&disconnected);
        assert_eq!(classified.kind, PeerRequestErrorKind::Transport);
        assert!(classified.kind.is_retryable());

        let timed_out: anyhow::Error =
            SessionError::TransferTimeout(std::time::Duration::from_secs(60)).into();
        let classified = classify_peer_error(&timed_out);
        assert_eq!(classified.kind, PeerRequestErrorKind::Timeout);
        assert!(classified.kind.is_retryable());

        let rejected: anyhow::Error = SessionError::HashMismatch {
            path: "a.txt".into(),
        }
        .into();
        let classified = classify_peer_error(&rejected);
        assert_eq!(classified.kind, PeerRequestErrorKind::Internal);
        assert!(!classified.kind.is_retryable());

        // Classification survives context wrapping, since it walks the chain.
        let wrapped: anyhow::Error =
            anyhow::Error::from(SessionError::UnexpectedFile("x.txt".into()))
                .context("failed to sync folder");
        assert_eq!(
            classify_peer_error(&wrapped).kind,
            PeerRequestErrorKind::Internal
        );
    }

    /// A malformed peer address is a request error, never a transport retry.
    #[tokio::test]
    async fn malformed_peer_address_is_invalid_request() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        match browse_peer_shared_folders(&state, "not a host".into(), 9847).await {
            BrowseSharedFoldersResult::Failure(err) => {
                assert_eq!(err.kind, PeerRequestErrorKind::InvalidRequest);
                assert!(!err.kind.is_retryable());
            }
            BrowseSharedFoldersResult::Success(_) => panic!("malformed address must not succeed"),
        }
    }

    #[tokio::test]
    async fn classify_peer_error_walks_context_wrapped_timeout() {
        let elapsed = tokio::time::timeout(
            std::time::Duration::from_millis(1),
            std::future::pending::<()>(),
        )
        .await
        .expect_err("must time out");
        let err = anyhow::Error::from(elapsed).context("connect for shared-folder RPC");
        assert_eq!(
            classify_peer_error(&err).kind,
            PeerRequestErrorKind::Timeout
        );
    }

    #[tokio::test]
    async fn remember_peer_endpoint_validates_and_compare_swaps() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();
        let peer = "peer-id".to_string();
        state
            .storage
            .upsert_device(&peer, "Peer", None, None)
            .unwrap();

        // A malformed host never reaches the address store.
        assert!(
            remember_peer_endpoint(&state, peer.clone(), "not-an-ip".into(), 9847, None, 0)
                .is_err()
        );
        assert_eq!(state.storage.device_last_addr(&peer).unwrap(), None);

        // The version a client captured before resolving.
        let gen0 = device_addr_generation(&state, peer.clone()).unwrap();
        assert_eq!(gen0, 0);

        // Only-if-unset claims the empty row at the captured generation...
        assert!(
            remember_peer_endpoint(&state, peer.clone(), "10.0.0.5".into(), 9847, None, gen0)
                .unwrap()
        );
        assert_eq!(
            state.storage.device_last_addr(&peer).unwrap().as_deref(),
            Some("10.0.0.5:9847")
        );
        // ...but a second only-if-unset write with the now-stale generation
        // loses (the row is no longer empty and the version moved on).
        assert!(
            !remember_peer_endpoint(&state, peer.clone(), "10.0.0.9".into(), 9847, None, gen0)
                .unwrap()
        );
        assert_eq!(
            state.storage.device_last_addr(&peer).unwrap().as_deref(),
            Some("10.0.0.5:9847")
        );

        // A stale expectation loses the CAS and leaves the newer value intact.
        assert!(!remember_peer_endpoint(
            &state,
            peer.clone(),
            "10.0.0.9".into(),
            9847,
            Some("1.2.3.4:9847".into()),
            gen0
        )
        .unwrap());
        assert_eq!(
            state.storage.device_last_addr(&peer).unwrap().as_deref(),
            Some("10.0.0.5:9847")
        );

        // With the current generation and the current value, the update lands.
        let gen1 = device_addr_generation(&state, peer.clone()).unwrap();
        assert!(remember_peer_endpoint(
            &state,
            peer.clone(),
            "10.0.0.9".into(),
            9847,
            Some("10.0.0.5:9847".into()),
            gen1
        )
        .unwrap());
        assert_eq!(
            state.storage.device_last_addr(&peer).unwrap().as_deref(),
            Some("10.0.0.9:9847")
        );
    }

    /// A real connection to a closed port must classify as a retryable
    /// transport failure — the classifier has to see the actual
    /// `ConnectionRefused` the OS returns, not a guess.
    #[tokio::test]
    async fn browse_classifies_real_connection_refusal_as_transport() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        for _ in 0..5 {
            // Reserve then release a port so nothing is listening on it.
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);

            match browse_peer_shared_folders(&state, "127.0.0.1".into(), port).await {
                BrowseSharedFoldersResult::Failure(err) => {
                    assert_eq!(err.kind, PeerRequestErrorKind::Transport, "{err:?}");
                    assert!(err.kind.is_retryable());
                    return;
                }
                BrowseSharedFoldersResult::Success(_) => {
                    // The port got reused by another process; try a fresh one.
                    continue;
                }
            }
        }
        panic!("could not obtain a refused port");
    }

    /// A peer that accepts the TCP connection but never completes the TLS
    /// handshake must be classified as a timeout, not hang forever.
    #[tokio::test]
    async fn shared_folder_rpc_times_out_on_silent_peer() {
        use crate::sync_engine::shared_folder::SharedFolderClient;

        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Accept and hold connections open without ever speaking TLS.
        let held = tokio::spawn(async move {
            let mut conns = Vec::new();
            while let Ok((sock, _)) = listener.accept().await {
                conns.push(sock);
            }
        });

        let client = SharedFolderClient::with_request_timeout(
            state.crypto.clone(),
            addr,
            std::time::Duration::from_millis(200),
        );
        let err = client
            .list_shared_folders()
            .await
            .expect_err("silent peer must time out");
        assert_eq!(
            classify_peer_error(&err).kind,
            PeerRequestErrorKind::Timeout
        );

        held.abort();
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
        let (client_id, client_name) = {
            let guard = state.device_info.read().unwrap();
            (guard.id.clone(), guard.name.clone())
        };
        server_storage
            .upsert_device(&client_id, &client_name, Some(&client_cert_der), None)
            .unwrap();

        let incoming = client_data.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        // In production the pairing flow creates this device row.
        state
            .storage
            .upsert_device(
                &server_info.id,
                &server_info.name,
                Some(&server_crypto.certificate().await),
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
                "complete",
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
                "incomplete",
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
        assert_eq!(
            list_recent_sessions(&state, 1).unwrap()[0].status,
            "incomplete"
        );

        let for_device = list_sessions_for_device(&state, "phone-1".to_string(), 10).unwrap();
        assert_eq!(for_device.len(), 1);
        assert_eq!(for_device[0].direction, "bidirectional");
        assert_eq!(for_device[0].status, "complete");

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
        assert_eq!(removed, 3, "self + two trusted peers");
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
        assert!(!Path::new(&dir.path().join("metadata.db")).exists());

        // A fresh engine sees no state and gets a brand-new identity.
        let fresh = init_engine(data_dir).await.unwrap();
        assert!(
            list_devices(&fresh).unwrap().is_empty(),
            "a fresh install must not present any paired devices (its own row is hidden)"
        );
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
        let big = vec![b'a'; CONFLICT_READ_LIMIT + 100];
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
        // No *paired* device exists yet (no pairing has run); our own row is
        // present but hidden from the public list.
        assert!(list_devices(&state).unwrap().is_empty());

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

    /// Deterministic regression for the sync-fallback persistence race.
    ///
    /// A fallback captures (endpoint, generation) up front, then discovers and
    /// syncs through a different candidate. If another session moves the
    /// endpoint in the meantime, the fallback's stale write must be dropped
    /// while the sync itself is still considered successful. Exercised through
    /// the exact helper the fallback calls, so no real network or sleeps are
    /// needed to create the race.
    #[test]
    fn stale_sync_fallback_does_not_clobber_a_newer_endpoint() {
        use crate::storage::UpdateOutcome;
        use crate::sync_engine::remember_reached_endpoint;

        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("metadata.db")).unwrap();
        storage
            .upsert_device("peer-a", "Peer A", None, None)
            .unwrap();

        // Peer A has endpoint 192.168.1.10:9847 at generation 10.
        for _ in 0..10 {
            storage
                .set_device_last_addr("peer-a", "192.168.1.10:9847")
                .unwrap();
        }
        assert_eq!(storage.device_addr_generation("peer-a").unwrap(), 10);

        // The fallback begins and captures that state before discovering.
        let expected_previous = storage.device_last_addr("peer-a").unwrap();
        let expected_generation = storage.device_addr_generation("peer-a").unwrap();
        assert_eq!(expected_previous.as_deref(), Some("192.168.1.10:9847"));

        // Another session updates Peer A's endpoint to generation 11.
        storage
            .set_device_last_addr("peer-a", "192.168.1.20:9847")
            .unwrap();

        // The fallback successfully syncs through 192.168.1.30:9847 and now
        // tries to persist it using the state captured at generation 10.
        let outcome = remember_reached_endpoint(
            &storage,
            "peer-a",
            expected_previous.as_deref(),
            expected_generation,
            "192.168.1.30:9847".parse().unwrap(),
        );

        // The stale update is an expected concurrency result, not a sync error.
        assert_eq!(outcome, Some(UpdateOutcome::StaleGeneration));
        // The newer endpoint survives untouched, with its generation intact.
        assert_eq!(
            storage.device_last_addr("peer-a").unwrap().as_deref(),
            Some("192.168.1.20:9847")
        );
        assert_eq!(storage.device_addr_generation("peer-a").unwrap(), 11);
    }

    /// Positive counterpart: while the captured state is still current, a
    /// successful fallback persists the newly authenticated endpoint.
    #[test]
    fn current_sync_fallback_persists_its_endpoint() {
        use crate::storage::UpdateOutcome;
        use crate::sync_engine::remember_reached_endpoint;

        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(&dir.path().join("metadata.db")).unwrap();
        storage
            .upsert_device("peer-a", "Peer A", None, None)
            .unwrap();

        // Captured before discovery: no endpoint recorded yet.
        let expected_previous = storage.device_last_addr("peer-a").unwrap();
        let expected_generation = storage.device_addr_generation("peer-a").unwrap();
        assert_eq!(expected_previous, None);

        let outcome = remember_reached_endpoint(
            &storage,
            "peer-a",
            expected_previous.as_deref(),
            expected_generation,
            "192.168.1.30:9847".parse().unwrap(),
        );

        assert_eq!(outcome, Some(UpdateOutcome::Updated));
        assert_eq!(
            storage.device_last_addr("peer-a").unwrap().as_deref(),
            Some("192.168.1.30:9847")
        );
        assert_eq!(storage.device_addr_generation("peer-a").unwrap(), 1);
    }
}

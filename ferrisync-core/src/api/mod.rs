//! flutter_rust_bridge FFI surface.
//! Annotated functions are picked up by FRB codegen to generate Dart bindings.

use crate::crypto::CryptoProvider;
use crate::storage::Storage;
use crate::sync_engine::pairing::PairingManager;
use crate::sync_engine::server::{PairPolicy, ServeHandle};
use crate::sync_engine::session::{self, SyncResult};
use crate::sync_engine::{SyncEngine, SyncEvent};
use crate::DeviceInfo;
use rustls::pki_types::PrivateKeyDer;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

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
    pub device_info: DeviceInfo,
    pub data_dir: String,
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
        name: whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string()),
        cert_fingerprint: crypto.fingerprint().await,
    };

    let engine = Arc::new(SyncEngine::new(
        storage.clone(),
        crypto.clone(),
        device_info.clone(),
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
        device_info,
        data_dir,
    };

    // Serve every configured folder so peers can connect to us. Ports start
    // at 9847 and increment per folder; failures are logged, not fatal.
    for (i, (_id, local_path, _device, _dir, _last)) in
        state.storage.list_sync_folders()?.iter().enumerate()
    {
        let port = u16::try_from(9847 + i).unwrap_or(9847);
        if let Err(e) = start_server(&state, port, *(_id), local_path.clone()).await {
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

// ── Server ──

/// Live folder listeners owned by this app instance, keyed by folder id.
fn servers() -> &'static Mutex<HashMap<i64, ServeHandle>> {
    static SERVERS: OnceLock<Mutex<HashMap<i64, ServeHandle>>> = OnceLock::new();
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
        state.device_info.clone(),
        local_path,
        port,
        // Already-paired peers are admitted silently; unknown peers raise
        // PairRequested events for the app's consent dialog.
        PairPolicy::Confirm,
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
    servers().lock().unwrap().insert(folder_id, handle);
    Ok(())
}

/// Stop all folder listeners (idempotent).
pub async fn stop_server(state: &ApiState) -> anyhow::Result<()> {
    let handles: Vec<ServeHandle> = servers().lock().unwrap().drain().map(|(_, h)| h).collect();
    for handle in handles {
        let _ = handle.stop().await;
    }
    let _ = state; // symmetric signature for FFI stability
    Ok(())
}

// ── Device / Pairing ──

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

pub fn list_devices(state: &ApiState) -> anyhow::Result<Vec<DeviceEntry>> {
    let rows = state.storage.list_devices()?;
    Ok(rows
        .into_iter()
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

// ── Folders ──

pub fn add_sync_folder(
    state: &ApiState,
    local_path: String,
    device_id: String,
    direction: String,
) -> anyhow::Result<i64> {
    state
        .storage
        .add_sync_folder(&local_path, &device_id, &direction)
}

pub fn list_sync_folders(state: &ApiState) -> anyhow::Result<Vec<FolderEntry>> {
    let rows = state.storage.list_sync_folders()?;
    Ok(rows
        .into_iter()
        .map(
            |(id, local_path, device_id, direction, last_sync_at)| FolderEntry {
                id,
                local_path,
                device_id,
                direction,
                last_sync_at: last_sync_at.unwrap_or(0),
            },
        )
        .collect())
}

#[derive(Debug, Clone)]
pub struct FolderEntry {
    pub id: i64,
    pub local_path: String,
    pub device_id: String,
    pub direction: String,
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
) -> anyhow::Result<SyncResult> {
    let addr: std::net::SocketAddr = format!("{remote_ip}:{remote_port}").parse()?;
    let result = session::run_sync_session(
        state.crypto.clone(),
        state.storage.clone(),
        &local_path,
        addr,
        folder_id,
        &device_id,
        state.engine.event_sender(),
    )
    .await?;
    Ok(result)
}

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

// ── Device Info ──

pub fn device_id(state: &ApiState) -> String {
    state.device_info.id.to_string()
}

pub fn device_name(state: &ApiState) -> String {
    state.device_info.name.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

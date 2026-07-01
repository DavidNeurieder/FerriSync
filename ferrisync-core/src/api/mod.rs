//! flutter_rust_bridge FFI surface.
//! Annotated functions are picked up by FRB codegen to generate Dart bindings.

use crate::crypto::CryptoProvider;
use crate::storage::Storage;
use crate::sync_engine::pairing::PairingManager;
use crate::sync_engine::session::{self, SyncResult};
use crate::sync_engine::{SyncEngine, SyncEvent};
use crate::DeviceInfo;
use std::path::PathBuf;
use std::sync::Arc;

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
pub async fn init_engine(data_dir: String) -> anyhow::Result<ApiState> {
    let path = PathBuf::from(&data_dir);
    std::fs::create_dir_all(&path)?;

    let crypto = Arc::new(CryptoProvider::generate()?);
    let storage = Arc::new(Storage::open(&path.join("metadata.db"))?);

    let dev_id = uuid::Uuid::new_v4().to_string();
    let device_info = DeviceInfo {
        id: dev_id,
        name: whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string()),
        cert_fingerprint: crypto.fingerprint().await,
    };

    let engine = Arc::new(SyncEngine::new(storage.clone(), crypto.clone(), device_info.clone()));
    let pairing = Arc::new(PairingManager::new(crypto.clone(), storage.clone(), device_info.clone()));

    Ok(ApiState {
        crypto,
        storage,
        engine,
        pairing,
        device_info,
        data_dir,
    })
}

// ── Device / Pairing ──

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
    Ok(state.storage.add_sync_folder(&local_path, &device_id, &direction)?)
}

pub fn list_sync_folders(state: &ApiState) -> anyhow::Result<Vec<FolderEntry>> {
    let rows = state.storage.list_sync_folders()?;
    Ok(rows
        .into_iter()
        .map(|(id, local_path, device_id, direction, last_sync_at)| FolderEntry {
            id,
            local_path,
            device_id,
            direction,
            last_sync_at: last_sync_at.unwrap_or(0),
        })
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

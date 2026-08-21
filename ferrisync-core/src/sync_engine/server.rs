use crate::crypto::CryptoProvider;
use crate::discovery::DiscoveryService;
use crate::storage::Storage;
use crate::sync_engine::session;
use crate::sync_engine::SyncEvent;
use crate::DeviceInfo;
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

/// A running folder server. Stop it with [`ServeHandle::stop`].
pub struct ServeHandle {
    pub folder: String,
    pub port: u16,
    shutdown_tx: watch::Sender<bool>,
    discovery: Option<DiscoveryService>,
    task: tokio::task::JoinHandle<()>,
}

impl ServeHandle {
    /// Signal the accept loop to exit, stop advertising, and wait for the
    /// listener task to finish. Already-connected sessions run to completion.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(discovery) = &self.discovery {
            discovery.shutdown();
        }
        let _ = self.task.await;
    }
}

/// Host `folder` for incoming pairing requests and sync sessions.
///
/// Binds `0.0.0.0:{port}` (port 0 picks a free one), registers the folder row
/// needed for sync history, and advertises the device on the LAN via mDNS
/// (advertisement failure is logged but does not prevent serving).
///
/// Returns a handle for stopping plus the receiver of sync events; drain it to
/// observe pushes/pulls/conflicts.
pub async fn serve_folder(
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
    folder: String,
    port: u16,
) -> Result<(ServeHandle, mpsc::Receiver<SyncEvent>)> {
    let addr: std::net::SocketAddr = format!("0.0.0.0:{port}")
        .parse()
        .with_context(|| format!("invalid bind address 0.0.0.0:{port}"))?;
    let folder_id = register_folder(&storage, &folder, &device_info)?;

    let discovery = match DiscoveryService::new(device_info.clone(), port) {
        Ok(disc) => {
            if let Err(e) = disc.advertise() {
                log::warn!("mDNS advertise failed: {e}");
                None
            } else {
                Some(disc)
            }
        }
        Err(e) => {
            log::warn!("mDNS init failed: {e}");
            None
        }
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (event_tx, event_rx) = mpsc::channel(256);
    let task = session::listen_for_sync(
        crypto,
        storage.clone(),
        addr,
        folder.clone(),
        folder_id,
        event_tx,
        device_info,
        shutdown_rx,
    )
    .await?;

    Ok((
        ServeHandle {
            folder,
            port,
            shutdown_tx,
            discovery,
            task,
        },
        event_rx,
    ))
}

/// Reuse an existing row for this path, or create one owned by this device
/// (sync history references the folder id).
fn register_folder(storage: &Storage, folder: &str, device_info: &DeviceInfo) -> Result<i64> {
    for (id, path, _device, _direction, _last_sync) in storage.list_sync_folders()? {
        if path == folder {
            return Ok(id);
        }
    }
    storage.upsert_device(&device_info.id, &device_info.name, None)?;
    storage.add_sync_folder(folder, &device_info.id, "bidirectional")
}

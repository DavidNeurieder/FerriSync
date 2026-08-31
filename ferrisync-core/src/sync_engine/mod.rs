pub mod bulk;
pub mod conflicts;
pub mod pairing;
pub mod server;
pub mod session;

use crate::crypto::CryptoProvider;
use crate::persistence::StateStore;
use crate::storage::Storage;
use crate::sync_engine::session::SyncResult;
use crate::DeviceInfo;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Clone)]
pub enum SyncEvent {
    Syncing {
        folder_id: String,
    },
    Idle,
    Error {
        message: String,
    },
    /// An unknown device asked to pair and is being held for confirmation.
    PairRequested {
        name: String,
        id: String,
    },
    /// A known device completed pairing (or was re-accepted).
    DevicePaired {
        name: String,
        id: String,
    },
    FilePulled {
        path: String,
        device: String,
    },
    FilePushed {
        path: String,
        device: String,
    },
    Conflict {
        path: String,
        winner: String,
        loser: String,
    },
    /// Live transfer progress for a running session. `files_total`/`bytes_total`
    /// are derived from the reconciled transfer plan, so percentages computed
    /// from `done/total` are honest for that session. `stage` names the current
    /// transfer phase ("starting", "uploading" or "downloading") so the UI can
    /// show what is happening, not just a bar.
    Progress {
        folder_id: String,
        stage: String,
        files_done: u64,
        files_total: u64,
        bytes_done: u64,
        bytes_total: u64,
    },
}

/// The sync engine: owns all subsystems and provides the high-level sync API.
pub struct SyncEngine {
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
    state_store: Arc<dyn StateStore>,
    event_tx: mpsc::Sender<SyncEvent>,
    event_rx: Mutex<mpsc::Receiver<SyncEvent>>,
}

impl std::fmt::Debug for SyncEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncEngine")
            .field("device_info", &self.device_info)
            .finish_non_exhaustive()
    }
}

impl SyncEngine {
    pub fn new(
        storage: Arc<Storage>,
        crypto: Arc<CryptoProvider>,
        device_info: DeviceInfo,
        state_store: Arc<dyn StateStore>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            storage,
            crypto,
            device_info,
            state_store,
            event_tx: tx,
            event_rx: Mutex::new(rx),
        }
    }

    pub fn event_sender(&self) -> mpsc::Sender<SyncEvent> {
        self.event_tx.clone()
    }

    pub async fn events(&self) -> mpsc::Receiver<SyncEvent> {
        let (tx, rx) = mpsc::channel(256);
        let mut old_rx = self.event_rx.lock().await;
        while let Ok(event) = old_rx.try_recv() {
            let _ = tx.send(event).await;
        }
        rx
    }

    pub fn storage(&self) -> &Arc<Storage> {
        &self.storage
    }

    pub fn crypto(&self) -> &Arc<CryptoProvider> {
        &self.crypto
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    pub fn state_store(&self) -> &Arc<dyn StateStore> {
        &self.state_store
    }

    /// Run a complete bidirectional sync session as the initiating peer.
    pub async fn run_sync(
        &self,
        local_path: &str,
        remote_addr: SocketAddr,
        folder_id: i64,
        device_id: &str,
    ) -> Result<SyncResult> {
        session::run_sync_session(
            self.crypto.clone(),
            self.storage.clone(),
            local_path,
            remote_addr,
            folder_id,
            device_id,
            self.event_tx.clone(),
            self.state_store.clone(),
        )
        .await
    }

    /// Sync every configured sync folder sequentially.
    pub async fn sync_all_folders(&self) -> Result<Vec<bulk::FolderOutcome>> {
        bulk::sync_all_folders(
            self.crypto.clone(),
            self.storage.clone(),
            self.event_tx.clone(),
            &self.device_info.id,
            self.state_store.clone(),
        )
        .await
    }

    /// Host a folder for incoming pairing requests and sync sessions.
    pub async fn serve_folder(
        &self,
        folder: String,
        port: u16,
        pair_policy: server::PairPolicy,
    ) -> Result<(server::ServeHandle, mpsc::Receiver<SyncEvent>)> {
        server::serve_folder(
            self.storage.clone(),
            self.crypto.clone(),
            self.device_info.clone(),
            folder,
            port,
            pair_policy,
            self.state_store.clone(),
        )
        .await
    }
}

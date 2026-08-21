pub mod pairing;
pub mod session;

use crate::crypto::CryptoProvider;
use crate::storage::Storage;
use crate::DeviceInfo;
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
}

/// Sync event bus.
#[derive(Debug)]
pub struct SyncEngine {
    #[allow(dead_code)]
    storage: Arc<Storage>,
    #[allow(dead_code)]
    crypto: Arc<CryptoProvider>,
    #[allow(dead_code)]
    device_info: DeviceInfo,
    event_tx: mpsc::Sender<SyncEvent>,
    event_rx: Mutex<mpsc::Receiver<SyncEvent>>,
}

impl SyncEngine {
    pub fn new(
        storage: Arc<Storage>,
        crypto: Arc<CryptoProvider>,
        device_info: DeviceInfo,
    ) -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            storage,
            crypto,
            device_info,
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
}

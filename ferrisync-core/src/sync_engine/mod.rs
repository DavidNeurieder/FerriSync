pub mod bulk;
pub mod conflicts;
pub mod pairing;
pub mod server;
pub mod session;
pub mod session_transfer;
pub mod shared_folder;

use crate::crypto::CryptoProvider;
use crate::persistence::StateStore;
use crate::storage::Storage;
use crate::sync_engine::session::SyncResult;
use crate::DeviceInfo;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

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
    /// A device requested pairing to one of our shared folders (held for
    /// owner approval).
    FolderPairRequested {
        name: String,
        id: String,
        folder: String,
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

/// Cooperative cancellation token for long-running sync work.
///
/// The token is cheaply cloneable and may be shared between the API boundary,
/// background watches, and in-flight session tasks. Cancellation is checked at
/// await boundaries and before persistence steps; work already committed by an
/// earlier step is never rolled back, but a cancelled session never returns a
/// successful result.
#[derive(Debug, Clone)]
pub struct SyncCancelToken {
    cancelled: Arc<AtomicBool>,
    changed: Arc<watch::Sender<bool>>,
}

impl Default for SyncCancelToken {
    fn default() -> Self {
        let (changed, _) = watch::channel(false);
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            changed: Arc::new(changed),
        }
    }
}

impl SyncCancelToken {
    /// Create a token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Signal cancellation to every clone of this token.
    pub fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            // A send error only means nobody is currently waiting; the flag
            // above is authoritative for later subscribers.
            let _ = self.changed.send(true);
        }
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Wait until cancellation is requested.
    ///
    /// The watch channel is registered before the flag is rechecked, so a
    /// cancellation racing this future cannot be missed.
    pub async fn cancelled(&self) {
        let mut rx = self.changed.subscribe();
        loop {
            if *rx.borrow_and_update() {
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    }

    /// Return an error when cancellation has already been requested.
    pub fn ensure_not_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(OperationCancelled.into())
        } else {
            Ok(())
        }
    }
}

/// Typed cancellation error for cooperative sync operations.
#[derive(Debug, thiserror::Error)]
#[error("operation cancelled")]
pub struct OperationCancelled;

/// Record an endpoint that was just reached (a successful sync or a validated
/// peer RPC), but only if the caller's captured state is still current.
///
/// `expected_previous`/`expected_generation` must be read *before* the work that
/// reached `addr`; the update is then a single atomic compare-and-set. If
/// another session moved the endpoint in the meantime, its newer value wins and
/// this stale write is dropped (`StaleGeneration`), never clobbering it.
///
/// Persistence problems are logged and swallowed: remembering the endpoint is
/// best-effort and must never turn an already-successful operation into a
/// reported failure. Returns the storage outcome when the write was attempted.
pub(crate) fn remember_reached_endpoint(
    storage: &Storage,
    device_id: &str,
    expected_previous: Option<&str>,
    expected_generation: u64,
    addr: SocketAddr,
) -> Option<crate::storage::UpdateOutcome> {
    match storage.update_peer_endpoint_if_current(
        device_id,
        expected_generation,
        expected_previous,
        &addr.to_string(),
    ) {
        Ok(outcome) => {
            match outcome {
                crate::storage::UpdateOutcome::Updated => {
                    log::info!("remembered endpoint {addr} for {device_id}")
                }
                crate::storage::UpdateOutcome::StaleGeneration => log::debug!(
                    "endpoint for {device_id} changed since it was captured; \
                     keeping the newer value"
                ),
            }
            Some(outcome)
        }
        Err(e) => {
            log::warn!("could not remember endpoint {addr} for {device_id}: {e}");
            None
        }
    }
}

/// The sync engine: owns all subsystems and provides the high-level sync API.
pub struct SyncEngine {
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
    state_store: Arc<dyn StateStore>,
    /// Ingress for producers (sync sessions, servers). Events are fanned out
    /// to broadcast subscribers so multiple consumers can observe them.
    event_tx: mpsc::Sender<SyncEvent>,
    broadcast_tx: broadcast::Sender<SyncEvent>,
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
        let (tx, mut rx) = mpsc::channel(256);
        let (broadcast_tx, _) = broadcast::channel(256);
        let fanout = broadcast_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                // Ignore send errors (no active subscribers).
                let _ = fanout.send(event);
            }
        });
        Self {
            storage,
            crypto,
            device_info,
            state_store,
            event_tx: tx,
            broadcast_tx,
        }
    }

    /// Clone of the ingress sender used by sync sessions and server tasks.
    pub fn event_sender(&self) -> mpsc::Sender<SyncEvent> {
        self.event_tx.clone()
    }

    /// Subscribe to engine events. Receives events emitted after subscription.
    pub fn subscribe_events(&self) -> broadcast::Receiver<SyncEvent> {
        self.broadcast_tx.subscribe()
    }

    /// Snapshot of currently queued events for polling consumers.
    ///
    /// Returns a receiver that will only receive events sent after this call.
    /// Prefer `subscribe_events()` for continuous observation; this method is
    /// kept for API compatibility.
    pub async fn events(&self) -> broadcast::Receiver<SyncEvent> {
        self.broadcast_tx.subscribe()
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
    /// With `dry_run`, dials the peer, exchanges indexes, and returns the
    /// reconciliation plan (as a `SyncResult`) without transferring files.
    pub async fn run_sync(
        &self,
        local_path: &str,
        remote_addr: SocketAddr,
        folder_id: i64,
        device_id: &str,
        dry_run: bool,
    ) -> Result<SyncResult> {
        self.run_sync_with_cancel(
            local_path,
            remote_addr,
            folder_id,
            device_id,
            dry_run,
            &SyncCancelToken::new(),
        )
        .await
    }

    /// Cancellation-aware variant of [`Self::run_sync`].
    ///
    /// The token is threaded into the session so cancellation can interrupt
    /// connection setup, handshake reads, chunk transfers, and finalization —
    /// not just the API boundary.
    pub async fn run_sync_with_cancel(
        &self,
        local_path: &str,
        remote_addr: SocketAddr,
        folder_id: i64,
        device_id: &str,
        dry_run: bool,
        cancel: &SyncCancelToken,
    ) -> Result<SyncResult> {
        cancel.ensure_not_cancelled()?;
        // The stored remote_path for this (folder, device) pair tells the
        // peer where its copy lives; the payload reaches the session so the
        // responder can relocate its serving root (§17).
        let remote_path = self
            .storage
            .folder_pair_remote_path(folder_id, device_id)?
            .map(|p| p.to_string());
        session::run_sync_session_with_cancel(
            self.crypto.clone(),
            self.storage.clone(),
            local_path,
            remote_addr,
            folder_id,
            device_id,
            self.event_tx.clone(),
            self.state_store.clone(),
            dry_run,
            remote_path.as_deref(),
            cancel,
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

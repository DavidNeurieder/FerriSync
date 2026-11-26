//! Always-on, folder-independent listener.
//!
//! Device pairing, shared-folder discovery and folder-pair requests all need a
//! socket to talk to. Previously that socket was the folder server's, so a
//! device with no shared folders could neither be discovered nor accept a
//! pairing. This listener owns those folder-independent RPCs; file sync still
//! runs on the per-folder sockets.

use crate::crypto::CryptoProvider;
use crate::discovery::DiscoveryService;
use crate::protocol::{frame_message, ErrorCode, ErrorMessage, SyncMessage};
use crate::storage::Storage;
use crate::sync_engine::bulk::DEFAULT_PORT;
use crate::sync_engine::server::{PairGate, PairPolicy};
use crate::sync_engine::session::{
    accept_tls_peer, handle_list_shared_folders, handle_pair_request,
    handle_request_folder_pair, read_request, MAX_SESSIONS,
};
use crate::sync_engine::SyncEvent;
use crate::DeviceInfo;
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

/// Port for the always-on control listener. Folder servers keep using 9847+
/// so existing installations do not change addresses.
pub const CONTROL_PORT: u16 = 9846;

/// A running control listener. Stop it with [`ControlHandle::stop`].
#[derive(Debug)]
pub struct ControlHandle {
    pub port: u16,
    gate: PairGate,
    shutdown_tx: watch::Sender<bool>,
    discovery_task: tokio::task::JoinHandle<()>,
    task: tokio::task::JoinHandle<()>,
}

impl ControlHandle {
    /// Signal the accept loop and mDNS advertisement to stop, then wait for
    /// both. Already-connected peers run to completion.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.discovery_task.await;
        let _ = self.task.await;
    }

    /// Devices held for confirmation by this listener.
    pub fn pending_pairings(&self) -> Vec<(String, String)> {
        self.gate.pending_pairings()
    }

    pub fn approve_pairing(&self, device_id: &str, device_name: &str) -> Result<()> {
        self.gate.approve_pairing(device_id, device_name)
    }

    pub fn deny_pairing(&self, device_id: &str) {
        self.gate.deny_pairing(device_id)
    }
}

/// Start the folder-less control listener.
///
/// Returns the handle and its event receiver; the caller is responsible for
/// forwarding those events into the shared engine stream, exactly as folder
/// servers do.
pub async fn serve_control(
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
    port: u16,
) -> Result<(ControlHandle, mpsc::Receiver<SyncEvent>)> {
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let discovery_shutdown = shutdown_rx.clone();
    let (event_tx, event_rx) = mpsc::channel(256);
    // Control pairing is always interactive; folder servers keep their own
    // policies (including Python's explicit auto-accept opt-in).
    let gate = PairGate::new(PairPolicy::Confirm, storage.clone(), event_tx.clone());

    let v6: SocketAddr = format!("[::]:{port}").parse()?;
    let v4: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let listener = match TcpListener::bind(v6).await {
        Ok(l) => l,
        Err(e) => {
            log::warn!("control dual-stack bind failed ({e}); falling back to 0.0.0.0");
            TcpListener::bind(v4).await?
        }
    };
    let bound_port = listener.local_addr()?.port();
    let name = device_info.name.clone();
    let task_device_info = device_info.clone();
    let task_gate = gate.clone();

    let task = tokio::spawn(async move {
        let concurrency = Arc::new(tokio::sync::Semaphore::new(MAX_SESSIONS));
        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => break,
                accepted = listener.accept() => {
                    match accepted {
                        Ok((tcp, _)) => {
                            let crypto = crypto.clone();
                            let storage = storage.clone();
                            let device_info = task_device_info.clone();
                            let gate = task_gate.clone();
                            let event_tx = event_tx.clone();
                            let concurrency = concurrency.clone();
                            tokio::spawn(async move {
                                let Ok(permit) = concurrency.try_acquire() else {
                                    log::warn!("control connection rejected: too many concurrent sessions");
                                    return;
                                };
                                let _quota = permit;
                                let Some((mut tls, peer_addr)) =
                                    accept_tls_peer(&crypto, tcp).await
                                else {
                                    return;
                                };
                                let Some(msg) =
                                    read_request(&mut tls, &crypto, &storage, &device_info, None).await
                                else {
                                    return;
                                };
                                match msg {
                                    SyncMessage::PairRequest(req) => {
                                        handle_pair_request(
                                            &mut tls,
                                            req,
                                            crypto,
                                            storage,
                                            device_info,
                                            gate,
                                            peer_addr,
                                            // Record where the peer serves
                                            // folders, not where we accepted the
                                            // pairing. Sync must never dial the
                                            // control listener; the folder guid
                                            // refinement can correct a non-default
                                            // port later.
                                            DEFAULT_PORT,
                                        )
                                        .await;
                                    }
                                    SyncMessage::ListSharedFolders => {
                                        handle_list_shared_folders(
                                            &mut tls,
                                            &storage,
                                            &device_info,
                                            peer_addr,
                                        )
                                        .await;
                                    }
                                    SyncMessage::RequestFolderPair(req) => {
                                        handle_request_folder_pair(
                                            &mut tls,
                                            req,
                                            &storage,
                                            &device_info,
                                            &gate,
                                            &event_tx,
                                        )
                                        .await;
                                    }
                                    SyncMessage::OfferFolderPair(info, peer_path) => {
                                        crate::sync_engine::session::handle_offer_folder_pair(
                                            &mut tls,
                                            info,
                                            peer_path,
                                            &storage,
                                            &device_info,
                                        )
                                        .await;
                                    }
                                    other => {
                                        log::warn!(
                                            "unexpected {:?} on control port; sync belongs on a folder port",
                                            std::mem::discriminant(&other)
                                        );
                                        let reply = SyncMessage::Error(ErrorMessage {
                                            code: ErrorCode::InternalError,
                                            message: "use the folder server for sync".into(),
                                        });
                                        if let Ok(framed) = frame_message(&reply) {
                                            let _ = tls.write_all(&framed).await;
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => log::error!("control accept error: {e}"),
                    }
                }
            }
        }
        log::info!("control listener on {bound_port} stopped");
    });

    let discovery_task = tokio::spawn(async move {
        let disc = match DiscoveryService::new_for_pairing(device_info, bound_port) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("control mDNS init failed: {e}");
                return;
            }
        };
        if let Err(e) = disc.advertise() {
            log::warn!("control mDNS advertise failed: {e}");
            return;
        }
        log::info!("advertising pairing endpoint for '{name}' on port {bound_port}");
        let mut shutdown = discovery_shutdown;
        let _ = shutdown.changed().await;
        disc.shutdown();
    });

    Ok((
        ControlHandle {
            port: bound_port,
            gate,
            shutdown_tx,
            discovery_task,
            task,
        },
        event_rx,
    ))
}

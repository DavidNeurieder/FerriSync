use crate::crypto::CryptoProvider;
use crate::discovery::DiscoveryService;
use crate::storage::Storage;
use crate::sync_engine::session;
use crate::sync_engine::SyncEvent;
use crate::DeviceInfo;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

/// How incoming pairing requests from *unknown* devices are handled.
///
/// Devices already present in the host's device table are always accepted
/// instantly, regardless of policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairPolicy {
    /// Accept every request (legacy TOFU behaviour).
    AutoAccept,
    /// Hold unknown devices until [`ServeHandle::approve_pairing`] is called.
    Confirm,
}

/// Reason string sent to clients whose pairing is being held.
pub const PENDING_REASON: &str = "pending confirmation";
/// Reason string sent to clients the operator explicitly denied.
pub const DENIED_REASON: &str = "pairing denied by host";

#[derive(Debug, Clone)]
struct PendingPairing {
    device_id: String,
    device_name: String,
    cert_der: Option<Vec<u8>>,
}

/// Shared state backing [`PairPolicy::Confirm`]: pending requests plus a
/// deny-list so denied devices are not re-queued while the client retries.
#[derive(Debug, Default)]
struct PairGateInner {
    pending: Vec<PendingPairing>,
    denied: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct PairGate {
    policy: PairPolicy,
    inner: Arc<std::sync::Mutex<PairGateInner>>,
    storage: Arc<Storage>,
    events: mpsc::Sender<SyncEvent>,
}

impl PairGate {
    pub(crate) fn new(
        policy: PairPolicy,
        storage: Arc<Storage>,
        events: mpsc::Sender<SyncEvent>,
    ) -> Self {
        Self {
            policy,
            inner: Arc::new(std::sync::Mutex::new(PairGateInner::default())),
            storage,
            events,
        }
    }

    pub(crate) fn storage(&self) -> &Arc<Storage> {
        &self.storage
    }

    /// Decide how to answer an incoming pairing request from `device_id`.
    /// `device_id` is the identity derived from the peer's TLS certificate by
    /// the caller; `cert_der` is the peer's actual certificate.
    pub(crate) async fn admit(&self, device_id: &str, device_name: &str, cert_der: Option<Vec<u8>>) -> Admission {
        // A device is "known" if its certificate fingerprint is already in
        // the device table. This ties authorization to the cryptographic
        // identity (the TLS certificate), not to a self-claimed id.
        let known = cert_der
            .as_deref()
            .map(|cert| {
                let fp = blake3::hash(cert);
                self.storage
                    .get_device_by_cert_fingerprint(fp.as_bytes())
                    .map(|opt| opt.is_some())
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if known || self.policy == PairPolicy::AutoAccept {
            return Admission::Accept;
        }
        let should_emit = {
            let mut inner = self.inner.lock().unwrap();
            if inner.denied.contains(device_id) {
                return Admission::Deny;
            }
            if !inner.pending.iter().any(|p| p.device_id == device_id) {
                inner.pending.push(PendingPairing {
                    device_id: device_id.to_string(),
                    device_name: device_name.to_string(),
                    cert_der,
                });
                true
            } else {
                false
            }
        };
        if should_emit {
            let _ = self
                .events
                .send(SyncEvent::PairRequested {
                    name: device_name.to_string(),
                    id: device_id.to_string(),
                })
                .await;
        }
        Admission::Hold
    }

    /// Record a successful pairing of an already-known or auto-accepted peer.
    pub(crate) async fn paired(&self, name: &str, id: &str) {
        let _ = self
            .events
            .send(SyncEvent::DevicePaired {
                name: name.to_string(),
                id: id.to_string(),
            })
            .await;
    }
}

/// Outcome of [`PairGate::admit`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    Accept,
    /// Held for confirmation; client receives `PENDING_REASON`.
    Hold,
    /// Operator denied this device earlier; client receives `DENIED_REASON`.
    Deny,
}

/// A running folder server. Stop it with [`ServeHandle::stop`].
pub struct ServeHandle {
    pub folder: String,
    pub port: u16,
    shutdown_tx: watch::Sender<bool>,
    discovery_task: tokio::task::JoinHandle<()>,
    gate: Option<PairGate>,
    task: tokio::task::JoinHandle<()>,
}

impl ServeHandle {
    /// Signal the accept loop to exit, stop advertising, and wait for the
    /// listener task to finish. Already-connected sessions run to completion.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.discovery_task.await;
        let _ = self.task.await;
    }

    fn require_gate(&self) -> Result<&PairGate> {
        self.gate
            .as_ref()
            .context("this server runs with auto-accept; no pairings to confirm")
    }

    /// Devices currently held for confirmation: `(name, device_id)` pairs.
    pub fn pending_pairings(&self) -> Result<Vec<(String, String)>> {
        let gate = self.require_gate()?;
        let inner = gate.inner.lock().unwrap();
        Ok(inner
            .pending
            .iter()
            .map(|p| (p.device_name.clone(), p.device_id.clone()))
            .collect())
    }

    /// Approve a held pairing. The device is written into the host's table so
    /// its next pairing attempt is accepted instantly.
    pub fn approve_pairing(&self, device_id: &str, device_name: &str) -> Result<()> {
        let gate = self.require_gate()?;
        // Extract the peer's TLS certificate that was captured during the hold.
        let cert_der = {
            let inner = gate.inner.lock().unwrap();
            inner
                .pending
                .iter()
                .find(|p| p.device_id == device_id)
                .and_then(|p| p.cert_der.clone())
        };
        gate.storage()
            .upsert_device(device_id, device_name, cert_der.as_deref(), None)?;
        let mut inner = gate.inner.lock().unwrap();
        inner.pending.retain(|p| p.device_id != device_id);
        Ok(())
    }

    /// Deny a held pairing and remember the choice for this server's lifetime.
    pub fn deny_pairing(&self, device_id: &str) -> Result<()> {
        let gate = self.require_gate()?;
        let mut inner = gate.inner.lock().unwrap();
        inner.pending.retain(|p| p.device_id != device_id);
        inner.denied.insert(device_id.to_string());
        Ok(())
    }
}

/// Host `folder` for incoming pairing requests and sync sessions.
///
/// Binds `0.0.0.0:{port}` (port 0 picks a free one), registers the folder row
/// needed for sync history, and advertises the device on the LAN via mDNS
/// (advertisement failure is logged but does not prevent serving).
///
/// Returns a handle for stopping plus the receiver of sync events; drain it to
/// observe pairing requests, pushes/pulls, and conflicts.
pub async fn serve_folder(
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
    folder: String,
    port: u16,
    pair_policy: PairPolicy,
    state_store: Arc<dyn crate::persistence::StateStore>,
) -> Result<(ServeHandle, mpsc::Receiver<SyncEvent>)> {
    // Prefer a dual-stack bind so loopback clients that resolve to ::1
    // (notably adbd reverse tunnels) can reach us; fall back to v4-only when
    // IPv6 is unavailable.
    let folder_id = register_folder(&storage, &folder, &device_info)?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (event_tx, event_rx) = mpsc::channel(256);
    let gate = PairGate::new(pair_policy, storage.clone(), event_tx.clone());

    let v6_addr: std::net::SocketAddr = format!("[::]:{port}").parse()?;
    let v4_addr: std::net::SocketAddr = format!("0.0.0.0:{port}").parse()?;

    let make_listener = |addr| {
        session::listen_for_sync(
            crypto.clone(),
            storage.clone(),
            addr,
            folder.clone(),
            folder_id,
            event_tx.clone(),
            device_info.clone(),
            shutdown_rx.clone(),
            gate.clone(),
            state_store.clone(),
        )
    };

    let (task, bound_addr) = match make_listener(v6_addr).await {
        Ok(pair) => pair,
        Err(bind_err) => {
            log::warn!("dual-stack bind failed ({bind_err}); falling back to 0.0.0.0");
            make_listener(v4_addr).await?
        }
    };
    let bound_port = bound_addr.port();

    // Advertise in the background: mDNS daemon startup can stall (notably on
    // Android emulators) and must never delay binding or the caller.
    let discovery_task = tokio::spawn(async move {
        let disc = match DiscoveryService::new(device_info, bound_port) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("mDNS init failed: {e}");
                return;
            }
        };
        if let Err(e) = disc.advertise() {
            log::warn!("mDNS advertise failed: {e}");
            return;
        }
        // Keep the advertisement alive until shutdown fires.
        let mut shutdown_rx = shutdown_rx;
        let _ = shutdown_rx.changed().await;
        disc.shutdown();
    });

    Ok((
        ServeHandle {
            folder,
            port: bound_port,
            shutdown_tx,
            discovery_task,
            gate: Some(gate),
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
    storage.upsert_device(&device_info.id, &device_info.name, None, None)?;
    storage.add_sync_folder(folder, &device_info.id, "bidirectional")
}

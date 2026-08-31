//! Bulk sync: run one-shot sync sessions for every configured sync folder.

use crate::crypto::CryptoProvider;
use crate::discovery::DiscoveryService;
use crate::persistence::StateStore;
use crate::storage::Storage;
use crate::sync_engine::session::{self, SyncResult};
use crate::sync_engine::SyncEvent;
use crate::DeviceInfo;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

pub const DEFAULT_PORT: u16 = 9847;

/// How long to browse mDNS for devices that have no recorded address.
const DISCOVERY_WINDOW: Duration = Duration::from_secs(2);

/// Result of one folder inside a bulk sync.
pub struct FolderOutcome {
    pub path: String,
    pub device_id: String,
    /// `None` when no address could be resolved (folder was skipped).
    pub addr: Option<SocketAddr>,
    /// `None` when the folder was skipped, otherwise the session result.
    pub result: Option<anyhow::Result<SyncResult>>,
}

/// Resolve the remote address for a folder's device from static records:
/// prefers the address recorded at pairing time (`last_addr`), falling back
/// to parsing the device id itself — legacy CLI rows use "ip[:port]" as the
/// device id. Does not touch the network beyond name resolution.
pub fn resolve_addr(device_id: &str, last_addr: Option<&str>) -> Option<SocketAddr> {
    if let Some(addr) = last_addr {
        if let Ok(parsed) = addr.parse::<SocketAddr>() {
            return Some(parsed);
        }
    }
    if device_id.contains(':') {
        device_id.parse().ok()
    } else {
        format!("{device_id}:{DEFAULT_PORT}").parse().ok()
    }
}

/// Resolve via [`resolve_addr`], then fall back to DNS for hostname-style ids
/// (e.g. "localhost"). Bounded so bogus ids cannot stall the bulk run.
async fn try_resolve(device_id: &str, last_addr: Option<&str>) -> Option<SocketAddr> {
    if let Some(addr) = resolve_addr(device_id, last_addr) {
        return Some(addr);
    }
    let target = match (last_addr, device_id) {
        (Some(host), _) if !host.contains(char::is_whitespace) => format!("{host}:{DEFAULT_PORT}"),
        (_, id) => format!("{id}:{DEFAULT_PORT}"),
    };
    tokio::time::timeout(Duration::from_millis(500), tokio::net::lookup_host(target))
        .await
        .ok()?
        .ok()?
        .find(|a| a.is_ipv4())
}

/// True when `addr` is one of this machine's own addresses (loopback included).
///
/// Uses the UDP-connect trick: the kernel picks the outgoing interface for the
/// target without sending any packet; if that interface's address equals the
/// target address, we are looking at ourselves.
pub fn is_own_address(addr: SocketAddr) -> bool {
    use std::net::UdpSocket;
    if addr.ip().is_loopback() {
        return true;
    }
    let host = SocketAddr::new(addr.ip(), 0);
    let Ok(sock) = UdpSocket::bind(host) else {
        return false;
    };
    sock.connect(addr).is_ok_and(|_| {
        sock.local_addr()
            .map(|local| local.ip() == addr.ip())
            .unwrap_or(false)
    })
}

/// Browse mDNS briefly and map discovered device ids to an address.
/// Peers advertising our own id are ignored.
async fn discover_addresses(seconds: u64, own_device_id: &str) -> HashMap<String, SocketAddr> {
    let mut found = HashMap::new();
    let info = DeviceInfo {
        id: "bulk-sync".into(),
        name: "ferrisync".into(),
        cert_fingerprint: Vec::new(),
    };
    let Ok(service) = DiscoveryService::new(info, DEFAULT_PORT) else {
        return found;
    };
    let Ok(mut rx) = service.browse() else {
        service.shutdown();
        return found;
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    while let Ok(Some(peer)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        if peer.id == own_device_id {
            continue;
        }
        if let (id, Some(addr)) = (peer.id, peer.addresses.first()) {
            found.entry(id).or_insert(*addr);
        }
    }
    service.shutdown();
    found
}

/// Browse mDNS briefly and return the address advertised by `device_id`.
/// Peers advertising our own id are ignored. Used as a freshness fallback
/// when a stored address no longer answers.
pub async fn discover_address_for(
    device_id: &str,
    own_device_id: &str,
    secs: u64,
) -> Option<SocketAddr> {
    discover_addresses(secs.max(1), own_device_id)
        .await
        .get(device_id)
        .copied()
}

/// Sync every configured sync folder sequentially.
///
/// Addresses come from pairing records, legacy ip-style device ids, DNS for
/// hostname-style ids, and finally a short mDNS browse for anything still
/// unresolved. A failing or unresolvable folder does not abort the rest;
/// per-folder results are returned in order. Successfully contacted
/// addresses are persisted back onto their device rows.
pub async fn sync_all_folders(
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    event_tx: mpsc::Sender<SyncEvent>,
    own_device_id: &str,
    state_store: Arc<dyn StateStore>,
) -> anyhow::Result<Vec<FolderOutcome>> {
    sync_all_folders_with(crypto, storage, event_tx, DISCOVERY_WINDOW, own_device_id, state_store).await
}

pub async fn sync_all_folders_with(
    crypto: Arc<CryptoProvider>,
    storage: Arc<Storage>,
    event_tx: mpsc::Sender<SyncEvent>,
    discovery_window: Duration,
    own_device_id: &str,
    state_store: Arc<dyn StateStore>,
) -> anyhow::Result<Vec<FolderOutcome>> {
    // Connected peers = every paired device other than ourselves. A served
    // (own-keyed) folder fans out to each of these.
    let peer_ids: Vec<String> = storage
        .list_devices()?
        .into_iter()
        .map(|(id, _, _)| id)
        .filter(|id| id != own_device_id)
        .collect();

    // Static resolution pass.
    let folders = storage.list_sync_folders()?;
    let mut resolved: Vec<Option<SocketAddr>> = Vec::with_capacity(folders.len());
    for (_id, _path, device_id, _dir, _last) in &folders {
        let last = storage.device_last_addr(device_id)?;
        resolved.push(try_resolve(device_id, last.as_deref()).await);
    }
    let mut peer_addrs: Vec<Option<SocketAddr>> = Vec::with_capacity(peer_ids.len());
    for id in &peer_ids {
        let last = storage.device_last_addr(id)?;
        peer_addrs.push(try_resolve(id, last.as_deref()).await);
    }

    // Discovery pass for whatever is still unresolved.
    if discovery_window > Duration::ZERO {
        let missing: Vec<&String> = folders
            .iter()
            .zip(&resolved)
            .filter(|(_, r)| r.is_none())
            .map(|((_i, _p, d, _, _), _)| d)
            .chain(
                peer_ids
                    .iter()
                    .zip(&peer_addrs)
                    .filter(|(_, a)| a.is_none())
                    .map(|(id, _)| id),
            )
            .collect();
        if !missing.is_empty() {
            let discovered = discover_addresses(discovery_window.as_secs(), own_device_id).await;
            for (idx, (_id, _path, device_id, _dir, _last)) in folders.iter().enumerate() {
                if resolved[idx].is_none() {
                    resolved[idx] = discovered.get(device_id).copied();
                }
            }
            for (idx, id) in peer_ids.iter().enumerate() {
                if peer_addrs[idx].is_none() {
                    peer_addrs[idx] = discovered.get(id).copied();
                }
            }
        }
    }

    // Session pass.
    let mut outcomes = Vec::new();
    for ((id, path, device_id, _direction, _last_sync), addr) in folders.iter().zip(&resolved) {
        if device_id == own_device_id {
            // Rows owned by ourselves exist because `serve` registers the
            // hosted folder for history bookkeeping. Sync it with every
            // connected peer (fan-out) so a bare `sync` keeps the folder in
            // sync across all devices rather than reporting a no-op.
            let mut contacted = false;
            for (peer_id, peer_addr) in peer_ids.iter().zip(&peer_addrs) {
                let Some(peer_addr) = peer_addr else { continue };
                if is_own_address(*peer_addr) {
                    continue;
                }
                contacted = true;
                let result = session::run_sync_session(
                    crypto.clone(),
                    storage.clone(),
                    path,
                    *peer_addr,
                    *id,
                    peer_id,
                    event_tx.clone(),
                    state_store.clone(),
                )
                .await;
                if result.is_ok() {
                    let _ = storage.set_device_last_addr(peer_id, &peer_addr.to_string());
                }
                outcomes.push(FolderOutcome {
                    path: path.clone(),
                    device_id: peer_id.clone(),
                    addr: Some(*peer_addr),
                    result: Some(result),
                });
            }
            if !contacted {
                // No reachable peer to keep the served folder in sync with.
                outcomes.push(FolderOutcome {
                    path: path.clone(),
                    device_id: device_id.clone(),
                    addr: None,
                    result: Some(Err(anyhow::anyhow!(
                        "no connected peers to sync served folder '{path}' with — pair or discover a device first"
                    ))),
                });
            }
            continue;
        }
        let Some(addr) = addr else {
            outcomes.push(FolderOutcome {
                path: path.clone(),
                device_id: device_id.clone(),
                addr: None,
                result: None,
            });
            continue;
        };
        if is_own_address(*addr) {
            outcomes.push(FolderOutcome {
                path: path.clone(),
                device_id: device_id.clone(),
                addr: Some(*addr),
                result: Some(Err(anyhow::anyhow!(
                    "{addr} is this machine — the device row points at us"
                ))),
            });
            continue;
        }
        let result = session::run_sync_session(
            crypto.clone(),
            storage.clone(),
            path,
            *addr,
            *id,
            device_id,
            event_tx.clone(),
            state_store.clone(),
        )
        .await;
        if result.is_ok() {
            let _ = storage.set_device_last_addr(device_id, &addr.to_string());
        }
        outcomes.push(FolderOutcome {
            path: path.clone(),
            device_id: device_id.clone(),
            addr: Some(*addr),
            result: Some(result),
        });
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_addr_wins() {
        assert_eq!(
            resolve_addr("uuid-123", Some("10.0.0.9:7000")),
            "10.0.0.9:7000".parse().ok()
        );
    }

    #[test]
    fn falls_back_to_device_id_as_ip() {
        assert_eq!(
            resolve_addr("192.168.1.5", None),
            "192.168.1.5:9847".parse().ok()
        );
        assert_eq!(
            resolve_addr("192.168.1.5:7000", None),
            "192.168.1.5:7000".parse().ok()
        );
    }

    #[test]
    fn unresolvable_device_yields_none() {
        assert_eq!(
            resolve_addr("b50b3a4f-282d-48fb-b311-8c9e8651cb4c", None),
            None
        );
        assert_eq!(resolve_addr("uuid", Some("not-an-addr")), None);
    }

    #[tokio::test]
    async fn localhost_resolves_via_dns_fallback() {
        let addr = try_resolve("localhost", None).await;
        assert_eq!(addr, Some(SocketAddr::from(([127, 0, 0, 1], 9847))));
    }

    #[tokio::test]
    async fn unknown_devices_are_skipped_not_failed() {
        let dir = tempfile::tempdir().unwrap();
        let crypto = Arc::new(CryptoProvider::generate().unwrap());
        let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
        storage
            .upsert_device("uuid-no-addr", "phone", None, None)
            .unwrap();
        storage
            .add_sync_folder("/does/not/matter", "uuid-no-addr", "bidirectional")
            .unwrap();

        let (event_tx, _event_rx) = mpsc::channel(256);
        let outcomes = sync_all_folders_with(
            crypto,
            storage.clone(),
            event_tx,
            Duration::ZERO,
            "self-uuid",
            Arc::new(crate::persistence::InMemoryStateStore::new()),
        )
        .await
        .unwrap();

        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].addr.is_none());
        assert!(outcomes[0].result.is_none());
    }

    #[tokio::test]
    async fn served_folder_without_peers_reports_no_connected_peers() {
        let dir = tempfile::tempdir().unwrap();
        let crypto = Arc::new(CryptoProvider::generate().unwrap());
        let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
        let own = "own-uuid";
        storage.upsert_device(own, "me", None, None).unwrap();
        storage
            .add_sync_folder("/served/here", own, "bidirectional")
            .unwrap();

        let (event_tx, _event_rx) = mpsc::channel(256);
        let outcomes = sync_all_folders_with(
            crypto,
            storage,
            event_tx,
            Duration::ZERO,
            own,
            Arc::new(crate::persistence::InMemoryStateStore::new()),
        )
        .await
        .unwrap();

        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].addr.is_none());
        let err = outcomes[0]
            .result
            .as_ref()
            .expect("labeled outcome present")
            .as_ref()
            .expect_err("a served folder with no peers must not run a session");
        assert!(
            format!("{err:#}").contains("no connected peers to sync served folder"),
            "{err:#}"
        );
    }

    #[tokio::test]
    async fn served_folder_fans_out_to_connected_peers() {
        let dir = tempfile::tempdir().unwrap();
        let crypto = Arc::new(CryptoProvider::generate().unwrap());
        let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
        let own = "own-uuid";
        storage.upsert_device(own, "me", None, None).unwrap();
        // Two paired peers, one with a recorded address and one without.
        storage
            .upsert_device("peer-a", "phone-a", None, Some("10.0.0.50:9847"))
            .unwrap();
        storage
            .upsert_device("peer-b", "phone-b", None, None)
            .unwrap();
        storage
            .add_sync_folder("/served/here", own, "bidirectional")
            .unwrap();

        let (event_tx, _event_rx) = mpsc::channel(256);
        let outcomes = sync_all_folders_with(
            crypto,
            storage,
            event_tx,
            Duration::ZERO,
            own,
            Arc::new(crate::persistence::InMemoryStateStore::new()),
        )
        .await
        .unwrap();

        // Only the peer with a resolvable address is attempted.
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].path, "/served/here");
        assert_eq!(outcomes[0].device_id, "peer-a");
        assert_eq!(
            outcomes[0].addr,
            Some(SocketAddr::from(([10, 0, 0, 50], 9847)))
        );
        assert!(outcomes[0].result.is_some());
    }
}

//! Bulk sync: run one-shot sync sessions for every configured sync folder.

use crate::crypto::{cert_to_device_id, CryptoProvider};
use crate::discovery::DiscoveryService;
use crate::persistence::StateStore;
use crate::storage::Storage;
use crate::sync_engine::session::{self, SyncResult};
use crate::sync_engine::SyncEvent;
use crate::transport::tcp::TcpTransport;
use crate::transport::TransportConnector;
use crate::DeviceInfo;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
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

/// Order a peer's advertised addresses into a deterministic list of usable
/// endpoint candidates: drop port-0 entries, de-duplicate, prefer IPv4 over
/// IPv6, then sort by address and port within each family. The first element is
/// the preferred endpoint; the rest are fallbacks to try, in order, when it is
/// unreachable.
///
/// Preferring IPv4 keeps the choice stable on dual-stack hosts and matches the
/// rest of the codebase, which dials IPv4-mapped peers as plain IPv4.
pub fn pick_advertised_addrs(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
    let mut out: Vec<SocketAddr> = addrs.iter().copied().filter(|a| a.port() != 0).collect();
    out.sort_by_key(|a| (!a.is_ipv4(), a.ip(), a.port()));
    out.dedup();
    out
}

/// Claim the process-wide, single in-flight discovery slot for `device_id`.
///
/// Returns `None` when a discovery for the same peer is already running.
/// Repeated pairings must not spawn unbounded duplicate mDNS probes; the
/// returned guard releases the slot on drop.
pub fn try_claim_discovery(device_id: &str) -> Option<DiscoveryClaim> {
    let mut claimed = in_flight_discoveries().lock().unwrap();
    if !claimed.insert(device_id.to_string()) {
        return None;
    }
    Some(DiscoveryClaim {
        device_id: device_id.to_string(),
    })
}

/// RAII guard for an in-flight discovery slot; see [`try_claim_discovery`].
pub struct DiscoveryClaim {
    device_id: String,
}

impl Drop for DiscoveryClaim {
    fn drop(&mut self) {
        if let Ok(mut claimed) = in_flight_discoveries().lock() {
            claimed.remove(&self.device_id);
        }
    }
}

fn in_flight_discoveries() -> &'static Mutex<HashSet<String>> {
    static SET: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Browse mDNS briefly and map discovered device ids to their ordered list of
/// endpoint candidates. Peers advertising our own id are ignored; all addresses
/// seen for a peer are accumulated before ordering, so the result does not
/// depend on advertisement ordering or duplicate advertisements.
async fn discover_candidates(
    seconds: u64,
    own_device_id: &str,
) -> HashMap<String, Vec<SocketAddr>> {
    let mut seen: HashMap<String, Vec<SocketAddr>> = HashMap::new();
    let info = DeviceInfo {
        id: "bulk-sync".into(),
        name: "ferrisync".into(),
        cert_fingerprint: Vec::new(),
    };
    let Ok(service) = DiscoveryService::new(info, DEFAULT_PORT) else {
        return HashMap::new();
    };
    let Ok(mut rx) = service.browse() else {
        service.shutdown();
        return HashMap::new();
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    while let Ok(Some(peer)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        // Identity match: mDNS advertises the cert-derived device id, so a peer
        // advertising a different id is a different device and is never used to
        // overwrite the expected peer's address. (The advertised id is only a
        // claim; the address is authenticated separately before it is trusted.)
        if peer.id == own_device_id {
            continue;
        }
        seen.entry(peer.id).or_default().extend(peer.addresses);
    }
    service.shutdown();
    seen.into_iter()
        .map(|(id, addrs)| (id, pick_advertised_addrs(&addrs)))
        .filter(|(_, addrs)| !addrs.is_empty())
        .collect()
}

/// Browse mDNS briefly and return `device_id`'s advertised endpoint candidates
/// in deterministic preference order (empty when none were seen). Used as a
/// freshness fallback when a stored address no longer answers.
pub async fn discover_candidates_for(
    device_id: &str,
    own_device_id: &str,
    secs: u64,
) -> Vec<SocketAddr> {
    discover_candidates(secs.max(1), own_device_id)
        .await
        .remove(device_id)
        .unwrap_or_default()
}

/// Browse mDNS briefly and return the most-preferred address advertised by
/// `device_id`, if any. See [`discover_candidates_for`] for the full list.
pub async fn discover_address_for(
    device_id: &str,
    own_device_id: &str,
    secs: u64,
) -> Option<SocketAddr> {
    discover_candidates_for(device_id, own_device_id, secs)
        .await
        .into_iter()
        .next()
}

/// How long a single candidate may take to prove its identity.
pub const IDENTITY_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Try each candidate endpoint in order, returning the first one that
/// authenticates as `expected_peer_id`.
///
/// A candidate counts as valid only when it completes the TLS handshake *and*
/// the certificate it presents derives `expected_peer_id`. A successful TCP/TLS
/// connection to an unrelated machine is therefore rejected rather than
/// recorded, and an unreachable preferred address falls through to the next
/// candidate instead of failing the whole resolution.
///
/// `probe_timeout` bounds each candidate independently; a hanging candidate does
/// not stall the ones after it.
pub async fn first_authenticated_candidate(
    crypto: Arc<CryptoProvider>,
    expected_peer_id: &str,
    candidates: &[SocketAddr],
    probe_timeout: Duration,
) -> Option<SocketAddr> {
    if candidates.is_empty() {
        return None;
    }
    let transport = TcpTransport::new(crypto);
    for addr in candidates {
        match tokio::time::timeout(probe_timeout, transport.connect(*addr)).await {
            Ok(Ok(conn)) => {
                let authenticated = conn
                    .peer_cert_der()
                    .map(|der| cert_to_device_id(&der) == expected_peer_id)
                    .unwrap_or(false);
                if authenticated {
                    return Some(*addr);
                }
                log::debug!(
                    "address {addr} answered but is not device {expected_peer_id}; skipping"
                );
            }
            Ok(Err(e)) => {
                log::debug!("candidate {addr} unreachable while authenticating: {e:#}");
            }
            Err(_) => {
                log::debug!("candidate {addr} timed out while authenticating");
            }
        }
    }
    None
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
    sync_all_folders_with(
        crypto,
        storage,
        event_tx,
        DISCOVERY_WINDOW,
        own_device_id,
        state_store,
    )
    .await
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
            let discovered = discover_candidates(discovery_window.as_secs(), own_device_id).await;
            for (idx, (_id, _path, device_id, _dir, _last)) in folders.iter().enumerate() {
                if resolved[idx].is_none() {
                    resolved[idx] = discovered.get(device_id).and_then(|a| a.first().copied());
                }
            }
            for (idx, id) in peer_ids.iter().enumerate() {
                if peer_addrs[idx].is_none() {
                    peer_addrs[idx] = discovered.get(id).and_then(|a| a.first().copied());
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
                let remote_path = storage.folder_pair_remote_path(*id, peer_id)?;
                // Capture the endpoint state before the session so a concurrent
                // pairing/discovery that moves it during the sync is preserved.
                // Best-effort: a read hiccup skips the write, it does not fail
                // the sync.
                let expected_previous = storage.device_last_addr(peer_id).unwrap_or(None);
                let expected_generation = storage.device_addr_generation(peer_id).unwrap_or(0);
                let result = session::run_sync_session(
                    crypto.clone(),
                    storage.clone(),
                    path,
                    *peer_addr,
                    *id,
                    peer_id,
                    event_tx.clone(),
                    state_store.clone(),
                    false,
                    remote_path.as_deref(),
                )
                .await;
                if result.is_ok() {
                    super::remember_reached_endpoint(
                        &storage,
                        peer_id,
                        expected_previous.as_deref(),
                        expected_generation,
                        *peer_addr,
                    );
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
        let remote_path = storage.folder_pair_remote_path(*id, device_id)?;
        // Capture the endpoint state before the session so a concurrent
        // pairing/discovery that moves it during the sync is preserved.
        // Best-effort: a read hiccup skips the write, it does not fail the sync.
        let expected_previous = storage.device_last_addr(device_id).unwrap_or(None);
        let expected_generation = storage.device_addr_generation(device_id).unwrap_or(0);
        let result = session::run_sync_session(
            crypto.clone(),
            storage.clone(),
            path,
            *addr,
            *id,
            device_id,
            event_tx.clone(),
            state_store.clone(),
            false,
            remote_path.as_deref(),
        )
        .await;
        if result.is_ok() {
            super::remember_reached_endpoint(
                &storage,
                device_id,
                expected_previous.as_deref(),
                expected_generation,
                *addr,
            );
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

    /// The preferred endpoint from a candidate list (its first element).
    fn preferred(addrs: &[SocketAddr]) -> Option<SocketAddr> {
        pick_advertised_addrs(addrs).into_iter().next()
    }

    #[test]
    fn advertised_address_prefers_ipv4_and_ignores_ordering() {
        let v6 = "[fe80::1]:9847".parse::<SocketAddr>().unwrap();
        let v4 = "192.168.1.7:9847".parse::<SocketAddr>().unwrap();
        // Same result regardless of the order the peer advertised them in.
        assert_eq!(preferred(&[v6, v4]), Some(v4));
        assert_eq!(preferred(&[v4, v6]), Some(v4));
    }

    #[test]
    fn advertised_address_skips_unusable_port_and_empty() {
        let dead = "192.168.1.7:0".parse::<SocketAddr>().unwrap();
        assert_eq!(preferred(&[dead]), None);
        assert_eq!(preferred(&[]), None);
    }

    #[test]
    fn advertised_address_is_deterministic_among_ipv4_candidates() {
        let a = "192.168.1.9:9847".parse::<SocketAddr>().unwrap();
        let b = "192.168.1.4:9848".parse::<SocketAddr>().unwrap();
        assert_eq!(preferred(&[a, b]), Some(b));
        assert_eq!(preferred(&[b, a]), Some(b));
    }

    #[test]
    fn advertised_candidates_are_ordered_deduplicated_and_valid() {
        let v6 = "[fe80::1]:9847".parse::<SocketAddr>().unwrap();
        let v4_low = "192.168.1.4:9848".parse::<SocketAddr>().unwrap();
        let v4_high = "192.168.1.9:9847".parse::<SocketAddr>().unwrap();
        let dead = "192.168.1.7:0".parse::<SocketAddr>().unwrap();

        // IPv4 before IPv6, then ascending by address/port; port-0 dropped and
        // duplicates collapsed. The result is independent of input order.
        let expected = vec![v4_low, v4_high, v6];
        assert_eq!(
            pick_advertised_addrs(&[v6, v4_high, dead, v4_low, v4_low]),
            expected
        );
        assert_eq!(
            pick_advertised_addrs(&[v4_low, v4_low, dead, v4_high, v6]),
            expected
        );
    }

    #[test]
    fn advertised_candidates_skip_unusable_port_and_empty() {
        let dead = "192.168.1.7:0".parse::<SocketAddr>().unwrap();
        assert!(pick_advertised_addrs(&[dead]).is_empty());
        assert!(pick_advertised_addrs(&[]).is_empty());
    }

    #[tokio::test]
    async fn authenticated_candidate_probe_returns_none_when_empty() {
        let crypto = Arc::new(CryptoProvider::generate().unwrap());
        assert_eq!(
            first_authenticated_candidate(crypto, "peer", &[], IDENTITY_PROBE_TIMEOUT).await,
            None
        );
    }

    #[test]
    fn discovery_claims_are_deduplicated_and_released() {
        let first = try_claim_discovery("peer-x").expect("first claim wins");
        // A concurrent refinement for the same peer must not start a second
        // mDNS probe.
        assert!(try_claim_discovery("peer-x").is_none());
        // A different peer is independent.
        let other = try_claim_discovery("peer-y").expect("other peer claim wins");
        drop(other);

        // Releasing the claim frees the slot for the same peer again.
        drop(first);
        assert!(try_claim_discovery("peer-x").is_some());
    }
}

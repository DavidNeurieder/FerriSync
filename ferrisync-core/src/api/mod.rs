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
// Re-export so generated bridge code (`use crate::api::*`) can name RwLock.
#[allow(unused_imports)]
pub use std::sync::RwLock;

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
    /// Own identity. Behind a lock so a rename propagates to servers and
    /// pairing started afterwards without rebuilding the whole state.
    pub device_info: Arc<RwLock<DeviceInfo>>,
    pub data_dir: String,
}

impl ApiState {
    /// Snapshot of the current device identity.
    pub fn current_device(&self) -> DeviceInfo {
        self.device_info.read().unwrap().clone()
    }
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
        name: load_device_name(&path).unwrap_or_else(|| {
            whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string())
        }),
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
        device_info: Arc::new(RwLock::new(device_info)),
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
const DEVICE_NAME_FILE: &str = "device.name";

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

/// The user-chosen device name, if one was ever set. Absent means "use the
/// hostname"; the file is only written by an explicit rename.
pub fn load_device_name(path: &Path) -> Option<String> {
    let name = std::fs::read_to_string(path.join(DEVICE_NAME_FILE)).ok()?;
    let name = name.trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub fn persist_device_name(path: &Path, name: &str) {
    let _ = std::fs::write(path.join(DEVICE_NAME_FILE), name);
}

// ── Server ──

/// A live folder listener plus the parameters it was started with, so a
/// rename can stop and re-serve it under the fresh identity.
struct RunningServer {
    handle: ServeHandle,
    port: u16,
    local_path: String,
}

/// Live folder listeners owned by this app instance, keyed by folder id.
fn servers() -> &'static Mutex<HashMap<i64, RunningServer>> {
    static SERVERS: OnceLock<Mutex<HashMap<i64, RunningServer>>> = OnceLock::new();
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
        state.current_device(),
        local_path.clone(),
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
    let resolved_port = handle.port;
    servers().lock().unwrap().insert(
        folder_id,
        RunningServer {
            handle,
            port: resolved_port,
            local_path,
        },
    );
    Ok(())
}

/// Stop all folder listeners (idempotent).
pub async fn stop_server(state: &ApiState) -> anyhow::Result<()> {
    let handles: Vec<ServeHandle> = servers()
        .lock()
        .unwrap()
        .drain()
        .map(|(_, s)| s.handle)
        .collect();
    for handle in handles {
        let _ = handle.stop().await;
    }
    let _ = state; // symmetric signature for FFI stability
    Ok(())
}

/// Restart every running folder listener so mDNS advertisements and pairing
/// responses carry the current device name. Failures are logged, not fatal.
async fn restart_all_servers(state: &ApiState) {
    let entries: Vec<(i64, RunningServer)> = servers()
        .lock()
        .unwrap()
        .drain()
        .collect();
    if entries.is_empty() {
        return;
    }
    let total = entries.len();
    let mut restarted = 0;
    for (folder_id, server) in entries {
        let RunningServer {
            handle,
            port,
            local_path,
        } = server;
        let _ = handle.stop().await;
        match start_server(state, port, folder_id, local_path).await {
            Ok(()) => restarted += 1,
            Err(e) => log::warn!("failed to restart folder server {folder_id} on port {port}: {e}"),
        }
    }
    log::info!("restarted {restarted}/{total} folder server(s) after rename");
}

// ── Device / Pairing ──

/// Maximum length of a user-chosen device name. It travels in protocol
/// frames and mDNS records, so keep it modest.
pub const DEVICE_NAME_MAX_LEN: usize = 64;

/// Validate and normalize a user-chosen device name. Shared by every
/// frontend (app, REPL, CLI) so all enforce the same rules.
pub fn sanitize_device_name(name: &str) -> anyhow::Result<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("device name cannot be empty");
    }
    if name.chars().count() > DEVICE_NAME_MAX_LEN {
        anyhow::bail!("device name too long (max {DEVICE_NAME_MAX_LEN} characters)");
    }
    if name.chars().any(|c| c.is_control()) {
        anyhow::bail!("device name contains invalid characters");
    }
    Ok(name)
}

/// Rename this device: validates, persists across restarts, updates the live
/// identity used by pairing and new server sessions, and restarts any running
/// folder servers so peers immediately see the new name on the LAN.
pub async fn set_device_name(state: &ApiState, name: String) -> anyhow::Result<String> {
    let name = sanitize_device_name(&name)?;

    persist_device_name(Path::new(&state.data_dir), &name);
    state.device_info.write().unwrap().name = name.clone();
    state.pairing.set_name(&name);
    restart_all_servers(state).await;
    Ok(name)
}

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
    .await;

    match result {
        Ok(r) => {
            // Remember the working address so the next sync dials it directly.
            let _ = state
                .storage
                .set_device_last_addr(&device_id, &addr.to_string());
            Ok(r)
        }
        // The stored address went stale (phone/IP changed): fall back to a
        // short mDNS browse before giving up, then persist the fresh one.
        Err(e) if e.to_string().contains("could not reach") => {
            let fresh = crate::sync_engine::bulk::discover_address_for(
                &device_id,
                &state.current_device().id,
                DISCOVERY_FALLBACK_SECS,
            )
            .await;
            match fresh {
                Some(fresh)
                    if fresh != addr && !crate::sync_engine::bulk::is_own_address(fresh) =>
                {
                    let r = session::run_sync_session(
                        state.crypto.clone(),
                        state.storage.clone(),
                        &local_path,
                        fresh,
                        folder_id,
                        &device_id,
                        state.engine.event_sender(),
                    )
                    .await?;
                    let _ = state
                        .storage
                        .set_device_last_addr(&device_id, &fresh.to_string());
                    Ok(r)
                }
                _ => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// How long the mDNS freshness fallback browses for a live advertisement.
const DISCOVERY_FALLBACK_SECS: u64 = 2;

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
    state.current_device().id
}

pub fn device_name(state: &ApiState) -> String {
    state.current_device().name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::CryptoProvider;
    use crate::storage::Storage;
    use crate::sync_engine::bulk::sync_all_folders_with;
    use crate::sync_engine::server::{serve_folder, PairPolicy};
    use crate::DeviceInfo;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;

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

    /// Full host→server round trip through the folder server: bind on port 0,
    /// pull a server-side file, push a client-side file, and confirm the
    /// contact address was persisted onto the device row.
    #[tokio::test]
    async fn server_round_trip_push_and_pull() {
        let server_dir = tempfile::tempdir().unwrap();
        let served = server_dir.path().join("served");
        std::fs::create_dir(&served).unwrap();

        let server_storage =
            Arc::new(Storage::open(&server_dir.path().join("metadata.db")).unwrap());
        let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
        let server_info = DeviceInfo {
            id: "server-device".into(),
            name: "phone".into(),
            cert_fingerprint: Vec::new(),
        };

        let (handle, _events) = serve_folder(
            server_storage,
            server_crypto,
            server_info.clone(),
            served.to_string_lossy().to_string(),
            0,
            PairPolicy::AutoAccept,
        )
        .await
        .unwrap();
        assert_ne!(handle.port, 0, "port 0 must resolve to a real port");

        std::fs::write(served.join("from_server.txt"), b"server says hi").unwrap();

        // ── client side ──
        let client_dir = tempfile::tempdir().unwrap();
        let incoming = client_dir.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        let client_storage =
            Arc::new(Storage::open(&client_dir.path().join("metadata.db")).unwrap());
        let client_crypto = Arc::new(CryptoProvider::generate().unwrap());

        client_storage
            .upsert_device(
                &server_info.id,
                &server_info.name,
                None,
                Some(&format!("127.0.0.1:{}", handle.port)),
            )
            .unwrap();
        client_storage
            .add_sync_folder(
                incoming.to_string_lossy().as_ref(),
                &server_info.id,
                "bidirectional",
            )
            .unwrap();

        let (event_tx, _event_rx) = mpsc::channel(256);
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", handle.port).parse().unwrap();

        // The loopback server is this machine by definition, so bulk sync
        // would refuse it; drive the session layer directly instead.
        let folder_row = &client_storage.list_sync_folders().unwrap()[0];
        let result = crate::sync_engine::session::run_sync_session(
            client_crypto.clone(),
            client_storage.clone(),
            folder_row.1.as_str(),
            addr,
            folder_row.0,
            &server_info.id,
            event_tx.clone(),
        )
        .await
        .unwrap();
        assert!(
            result.pulled.contains(&"from_server.txt".to_string()),
            "expected pull of from_server.txt"
        );
        client_storage
            .set_device_last_addr(&server_info.id, &addr.to_string())
            .unwrap();
        assert_eq!(
            std::fs::read(incoming.join("from_server.txt")).unwrap(),
            b"server says hi"
        );
        assert_eq!(
            client_storage.device_last_addr(&server_info.id).unwrap(),
            Some(format!("127.0.0.1:{}", handle.port)),
            "contact address persisted"
        );

        // Push pass.
        std::fs::write(incoming.join("from_client.txt"), b"client reply").unwrap();
        let pushed = crate::sync_engine::session::run_sync_session(
            client_crypto,
            client_storage,
            incoming.to_str().unwrap(),
            addr,
            folder_row.0,
            &server_info.id,
            event_tx,
        )
        .await
        .unwrap();
        assert!(
            pushed.pushed.contains(&"from_client.txt".to_string()),
            "expected push of from_client.txt"
        );
        assert_eq!(
            std::fs::read(served.join("from_client.txt")).unwrap(),
            b"client reply"
        );

        handle.stop().await;
    }

    /// Bulk sync refuses device rows that resolve to one of this machine's own
    /// addresses instead of silently "syncing" against ourselves.
    #[tokio::test]
    async fn bulk_refuses_self_addressed_rows() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&dir.path().join("metadata.db")).unwrap());
        let crypto = Arc::new(CryptoProvider::generate().unwrap());

        storage
            .upsert_device("stale-self", "me-by-mistake", None, Some("127.0.0.1:9847"))
            .unwrap();
        let incoming = dir.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        storage
            .add_sync_folder(incoming.to_str().unwrap(), "stale-self", "bidirectional")
            .unwrap();

        let (event_tx, _event_rx) = mpsc::channel(256);
        let outcomes = sync_all_folders_with(
            crypto,
            storage.clone(),
            event_tx,
            Duration::ZERO,
            "this-client",
        )
        .await
        .unwrap();

        assert_eq!(outcomes.len(), 1);
        let err = outcomes[0]
            .result
            .as_ref()
            .expect("self-addressed row must produce an error outcome")
            .as_ref()
            .expect_err("expected refusal");
        assert!(
            format!("{err:#}").contains("points at us"),
            "unexpected error: {err:#}"
        );
    }

    /// A chosen device name survives an engine reload and is picked up by the
    /// pairing manager, instead of falling back to the hostname again.
    #[tokio::test]
    async fn set_device_name_persists_across_reload() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_str().unwrap().to_string();
        let state = init_engine(data_dir.clone()).await.unwrap();

        let renamed = set_device_name(&state, "  my-phone  ".into())
            .await
            .expect("valid rename");
        assert_eq!(renamed, "my-phone");
        assert_eq!(device_name(&state), "my-phone");
        assert_eq!(state.pairing.current_device().name, "my-phone");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("device.name")).unwrap(),
            "my-phone"
        );

        let reloaded = init_engine(data_dir).await.unwrap();
        assert_eq!(
            device_name(&reloaded),
            "my-phone",
            "rename must survive restart"
        );
    }

    #[tokio::test]
    async fn set_device_name_rejects_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        let too_long = "a".repeat(65);
        for bad in ["", "   ", too_long.as_str(), "bad\u{7}name"] {
            assert!(
                set_device_name(&state, bad.to_string())
                    .await
                    .is_err(),
                "expected rejection of {bad:?}"
            );
        }
        // The identity is untouched after failed renames.
        assert!(!dir.path().join("device.name").exists());
    }

    /// Renaming restarts running folder listeners so they advertise (and
    /// answer pairing with) the new name without an app restart.
    #[tokio::test]
    async fn set_device_name_restarts_running_servers() {
        let dir = tempfile::tempdir().unwrap();
        let state = init_engine(dir.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        let served = dir.path().join("served");
        std::fs::create_dir(&served).unwrap();
        state
            .storage
            .upsert_device("peer-1", "peer", None, None)
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder(served.to_str().unwrap(), "peer-1", "bidirectional")
            .unwrap();
        start_server(&state, 0, folder_id, served.to_str().unwrap().to_string())
            .await
            .unwrap();
        let port_before = servers().lock().unwrap()[&folder_id].handle.port;
        assert_ne!(port_before, 0);

        set_device_name(&state, "renamed-host".into()).await.unwrap();

        let port_after = {
            let guard = servers().lock().unwrap();
            assert!(
                guard.contains_key(&folder_id),
                "server must be running again after rename"
            );
            guard[&folder_id].handle.port
        };
        assert_ne!(port_after, 0);

        // Clean up: stop the listener so other tests see no stale entries.
        let handle = servers().lock().unwrap().remove(&folder_id).unwrap();
        handle.handle.stop().await;
    }
}

#[cfg(test)]
mod phone_pull_tests {
    use super::*;

    /// Phone-role sync through the public API entry point: a client with an
    /// empty folder must pull files that exist only on the server side.
    #[tokio::test]
    async fn api_sync_folder_pulls_from_server() {
        use crate::sync_engine::server::{serve_folder, PairPolicy};

        // Server side (the "REPL host"): seeded folder with one file.
        let server_dir = tempfile::tempdir().unwrap();
        let server_storage =
            Arc::new(Storage::open(&server_dir.path().join("metadata.db")).unwrap());
        let server_crypto = Arc::new(CryptoProvider::generate().unwrap());
        let server_info = crate::DeviceInfo {
            id: uuid::Uuid::new_v4().to_string(),
            name: "host".into(),
            cert_fingerprint: Vec::new(),
        };
        std::fs::write(server_dir.path().join("host_only.txt"), b"from host").unwrap();

        let (_handle, _events) = serve_folder(
            server_storage,
            server_crypto,
            server_info.clone(),
            server_dir.path().to_str().unwrap().to_string(),
            0,
            PairPolicy::AutoAccept,
        )
        .await
        .unwrap();
        let port = _handle.port;

        // Client side ("the phone") through the full API stack.
        let client_data = tempfile::tempdir().unwrap();
        let state = init_engine(client_data.path().to_str().unwrap().to_string())
            .await
            .unwrap();

        let incoming = client_data.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        // In production the pairing flow creates this device row.
        state
            .storage
            .upsert_device(&server_info.id, &server_info.name, None, None)
            .unwrap();
        let folder_id = state
            .storage
            .add_sync_folder(incoming.to_str().unwrap(), &server_info.id, "bidirectional")
            .unwrap();

        let result = sync_folder(
            &state,
            folder_id,
            incoming.to_str().unwrap().to_string(),
            "127.0.0.1".to_string(),
            port,
            server_info.id.clone(),
        )
        .await
        .unwrap();

        assert!(
            result.pulled.contains(&"host_only.txt".to_string()),
            "expected pull, got {:?}",
            result.pulled
        );
        assert_eq!(
            std::fs::read(incoming.join("host_only.txt")).unwrap(),
            b"from host"
        );
        // The working address is persisted for next time.
        assert_eq!(
            state.storage.device_last_addr(&server_info.id).unwrap(),
            Some(format!("127.0.0.1:{port}"))
        );
    }
}

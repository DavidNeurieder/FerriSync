//! Minimal example: configure → pair → sync a folder.
//!
//! This example demonstrates the **public API** of `ferrisync-core` without
//! reaching into any internal module.
//!
//! ```text
//! configure engine
//!       ↓
//! discover / connect (pairing)
//!       ↓
//! sync folder
//!       ↓
//! receive progress / result
//! ```
//!
//! Run with:
//!
//! ```sh
//! cargo run --example minimal_sync -- /path/to/sync/folder 192.168.1.42:9847
//! ```

use ferrisync_core::{
    load_device_name, CryptoProvider, DeviceInfo, PairingManager, Storage,
    SyncEngine,
};
use ferrisync_core::persistence::InMemoryStateStore;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Stable per-data-dir identity: the TLS keypair is persisted so paired
/// devices recognize us across restarts; the device id is derived from the
/// certificate fingerprint.
async fn load_or_create_crypto(data: &Path) -> anyhow::Result<Arc<CryptoProvider>> {
    std::fs::create_dir_all(data)?;
    let cert_path = data.join("cert.der");
    let key_path = data.join("key.der");

    if cert_path.exists() && key_path.exists() {
        let cert_der = std::fs::read(&cert_path)?;
        let key_der = std::fs::read(&key_path)?;
        let fingerprint = blake3::hash(&cert_der).as_bytes().to_vec();
        return Ok(Arc::new(CryptoProvider::load(cert_der, key_der, fingerprint)?));
    }

    let crypto = CryptoProvider::generate()?;
    let cert = crypto.certificate().await;
    std::fs::write(&cert_path, cert.as_ref())?;
    std::fs::write(&key_path, crypto.private_key().await.secret_der())?;
    Ok(Arc::new(crypto))
}

fn device_id_from_fingerprint(fingerprint: &[u8]) -> String {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&fingerprint[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let folder = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("/tmp/ferrisync-demo");
    let peer_addr: SocketAddr = args
        .get(2)
        .map(|s| s.parse())
        .transpose()
        .map_err(|_| anyhow::anyhow!("second argument must be ip:port"))?
        .unwrap_or("127.0.0.1:9847".parse().unwrap());

    // ── 1. Configure identity ──────────────────────────────────────
    let data_dir = std::env::temp_dir().join("ferrisync-example");

    let crypto = load_or_create_crypto(&data_dir).await?;
    let storage = Arc::new(Storage::open(&data_dir.join("metadata.db"))?);
    let state_store = Arc::new(InMemoryStateStore::new());

    let cert_fingerprint = crypto.fingerprint().await;
    let device_info = DeviceInfo {
        id: device_id_from_fingerprint(&cert_fingerprint),
        name: load_device_name(&data_dir).unwrap_or_else(|| "example-node".to_string()),
        cert_fingerprint,
    };

    println!("Device ID:  {}", device_info.id);
    println!("Device name: {}", device_info.name);

    // ── 2. Create the sync engine ──────────────────────────────────
    let engine = SyncEngine::new(
        storage.clone(),
        crypto.clone(),
        device_info.clone(),
        state_store,
    );

    // ── 3. Pair with the peer ──────────────────────────────────────
    let pairing = PairingManager::new(crypto.clone(), storage.clone(), device_info.clone());
    println!("Pairing with {peer_addr}...");
    match pairing.pair_with(peer_addr).await {
        Ok(peer) => println!("Paired with {} ({})", peer.name, peer.id),
        Err(e) => {
            eprintln!("Pairing failed (is the peer running?): {e}");
            eprintln!("Continuing anyway — if already paired, this is fine.");
        }
    }

    // ── 4. Set up sync folder ──────────────────────────────────────
    let folder_path = PathBuf::from(folder);
    std::fs::create_dir_all(&folder_path)?;
    let folder_id = storage.add_sync_folder(
        folder_path.to_str().unwrap_or(folder),
        &peer_addr.to_string(),
        "bidirectional",
    )?;
    println!("Sync folder registered (id={folder_id}): {}", folder_path.display());

    // ── 5. Sync ────────────────────────────────────────────────────
    println!("Syncing with {peer_addr}...");
    match engine.run_sync(folder, peer_addr, folder_id, &peer_addr.to_string(), false).await {
        Ok(result) => {
            println!("Sync complete!");
            println!("  Pushed:   {} files", result.pushed.len());
            println!("  Pulled:   {} files", result.pulled.len());
            println!("  Conflicts: {} files", result.conflicts.len());
        }
        Err(e) => {
            eprintln!("Sync failed: {e}");
        }
    }

    // ── 6. (Optional) Serve mode ───────────────────────────────────
    // Uncomment the block below to also host the folder for incoming peers:
    //
    // let (server, mut events) = engine.serve_folder(
    //     folder_path.to_str().unwrap_or(folder).to_string(),
    //     9847,
    //     PairPolicy::Confirm,
    // ).await?;
    // println!("Serving on 0.0.0.0:{}", server.port);
    // while let Some(event) = events.recv().await {
    //     println!("Event: {event:?}");
    // }
    // server.stop().await;

    Ok(())
}

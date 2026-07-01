use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::SyncEngine;
use std::sync::Arc;

pub async fn run(
    folder: String,
    device: String,
    storage: Arc<Storage>,
    engine: Arc<SyncEngine>,
) -> anyhow::Result<()> {
    let folder_id = storage.add_sync_folder(&folder, &device, "bidirectional")?;
    let addr: std::net::SocketAddr = format!("{device}:9847")
        .parse()
        .map_err(|_| anyhow::anyhow!("device must be an IP:port, got {device}"))?;
    println!("Syncing {folder} with device {addr}...");
    engine.sync_folder(folder_id, &folder, addr, &device).await?;
    println!("Sync complete.");
    Ok(())
}

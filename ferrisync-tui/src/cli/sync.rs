use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use std::sync::Arc;

pub async fn run(
    folder: String,
    device: String,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
) -> anyhow::Result<()> {
    let folder_id = storage.add_sync_folder(&folder, &device, "bidirectional")?;
    let addr: std::net::SocketAddr = format!("{device}:9847")
        .parse()
        .map_err(|_| anyhow::anyhow!("device must be an IP:port, got {device}"))?;
    println!("Syncing {folder} with device {addr}...");
    let (event_tx, _) = tokio::sync::mpsc::channel(256);
    let result = session::run_sync_session(
        crypto,
        storage,
        &folder,
        addr,
        folder_id,
        &device,
        event_tx,
    )
    .await?;
    println!("Sync complete. Pushed: {}, Pulled: {}",
        result.pushed.len(),
        result.pulled.len(),
    );
    Ok(())
}

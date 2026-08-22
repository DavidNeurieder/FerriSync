use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use std::sync::Arc;

use super::watch::get_or_create_folder;
use super::{ensure_device, parse_device};

pub async fn run(
    folder: String,
    device: String,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
) -> anyhow::Result<()> {
    ensure_device(&storage, &device)?;
    let folder_id = get_or_create_folder(&storage, &folder, &device)?;
    let addr = parse_device(&device, super::DEFAULT_PORT)?;
    println!("Syncing {folder} with device {addr}...");
    let (event_tx, _) = tokio::sync::mpsc::channel(256);
    let result =
        session::run_sync_session(crypto, storage, &folder, addr, folder_id, &device, event_tx)
            .await?;
    println!(
        "Sync complete. Pushed: {}, Pulled: {}",
        result.pushed.len(),
        result.pulled.len(),
    );
    Ok(())
}

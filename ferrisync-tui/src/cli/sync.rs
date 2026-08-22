use anyhow::bail;
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::bulk;
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

/// Sync every configured sync folder against its known device address.
pub async fn run_all(storage: Arc<Storage>, crypto: Arc<CryptoProvider>) -> anyhow::Result<()> {
    let (event_tx, _) = tokio::sync::mpsc::channel(256);
    let outcomes = bulk::sync_all_folders(crypto, storage, event_tx).await?;
    if outcomes.is_empty() {
        println!("No sync folders configured.");
        return Ok(());
    }

    let mut synced = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    for outcome in &outcomes {
        match (&outcome.addr, &outcome.result) {
            (None, _) => {
                skipped += 1;
                println!(
                    "Skipped {} — no known address for device {}; pair or discover first.",
                    outcome.path, outcome.device_id
                );
            }
            (Some(addr), Some(Ok(result))) => {
                synced += 1;
                println!(
                    "Synced {path} with {addr}. Pushed: {}, Pulled: {}",
                    result.pushed.len(),
                    result.pulled.len(),
                    path = outcome.path,
                );
            }
            (Some(_), Some(Err(e))) => {
                failed += 1;
                println!("Failed to sync {}: {e}", outcome.path);
            }
            (Some(_), None) => unreachable!("session ran but produced no result"),
        }
    }

    let summary = format!("Done: {synced} synced, {failed} failed, {skipped} skipped.");
    if synced == 0 && failed + skipped > 0 {
        bail!("{summary}");
    }
    println!("{summary}");
    Ok(())
}

/// Shared with the one-shot CLI subcommand: dispatch single vs bulk.
pub async fn run_dispatch(
    folder: Option<String>,
    device: Option<String>,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
) -> anyhow::Result<()> {
    match (folder, device) {
        (Some(folder), Some(device)) => run(folder, device, storage, crypto).await,
        (None, None) => run_all(storage, crypto).await,
        _ => anyhow::bail!("usage: sync [<folder> --device <ip[:port]>]"),
    }
}

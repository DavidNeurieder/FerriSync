use anyhow::bail;
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::bulk;
use ferrisync_core::sync_engine::session;
use std::sync::Arc;

use super::ensure_device;
use super::watch::get_or_create_folder;

pub async fn run(
    folder: String,
    device: String,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    own_device_id: &str,
) -> anyhow::Result<()> {
    let (row_device, resolved) = super::resolve_device_key(&storage, &device, own_device_id)?;
    if row_device == device {
        // Legacy ip-keyed row: make sure the device exists for the FK.
        ensure_device(&storage, &row_device)?;
    }
    let folder_id = get_or_create_folder(&storage, &folder, &row_device)?;
    let Some(addr) = resolved else {
        bail!("{row_device} has no recorded address yet — run 'discover', or have it pair again");
    };
    println!("Syncing {folder} with {addr}...");
    let (event_tx, _) = tokio::sync::mpsc::channel(256);
    let result = session::run_sync_session(
        crypto,
        storage,
        &folder,
        addr,
        folder_id,
        &row_device,
        event_tx,
    )
    .await?;
    println!(
        "Sync complete. Pushed: {}, Pulled: {}",
        result.pushed.len(),
        result.pulled.len(),
    );
    Ok(())
}

/// Sync every configured sync folder against its known device address.
pub async fn run_all(
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    own_device_id: &str,
) -> anyhow::Result<()> {
    let (event_tx, _) = tokio::sync::mpsc::channel(256);
    let outcomes = bulk::sync_all_folders(crypto, storage, event_tx, own_device_id).await?;
    if outcomes.is_empty() {
        println!("No sync folders configured.");
        return Ok(());
    }

    let mut synced = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut local = 0usize;
    for outcome in &outcomes {
        if outcome.device_id == own_device_id {
            local += 1;
            println!(
                "Local {} — hosted on this machine; attach a remote with: sync <folder> --device <name|uuid>",
                outcome.path
            );
            continue;
        }
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
                let loopback_hint = if addr.ip().is_loopback() {
                    " (warning: loopback — is this row pointing at this machine?)"
                } else {
                    ""
                };
                println!(
                    "Synced {path} with {addr}. Pushed: {}, Pulled: {}{loopback_hint}",
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
    if local > 0 {
        println!("{summary} ({local} hosted locally)");
    } else {
        println!("{summary}");
    }
    Ok(())
}

/// Shared with the one-shot CLI subcommand: dispatch single vs bulk.
pub async fn run_dispatch(
    folder: Option<String>,
    device: Option<String>,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    own_device_id: &str,
) -> anyhow::Result<()> {
    match (folder, device) {
        (Some(folder), Some(device)) => run(folder, device, storage, crypto, own_device_id).await,
        (None, None) => run_all(storage, crypto, own_device_id).await,
        _ => anyhow::bail!("usage: sync [<folder> --device <ip[:port]|name|uuid>]"),
    }
}

use anyhow::bail;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::storage::Storage;
use ferrisync_core::{CryptoProvider, SyncEngine};
use std::sync::Arc;

use super::ensure_device;
use super::watch::get_or_create_folder;

pub async fn run(
    folder: String,
    device: String,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    own_device_id: &str,
    wait_secs: u64,
) -> anyhow::Result<()> {
    let (row_device, resolved) = super::resolve_device_key(&storage, &device, own_device_id)?;
    if row_device == device {
        // Legacy ip-keyed row: make sure the device exists for the FK.
        ensure_device(&storage, &row_device)?;
    }
    // Adopt served-bookkeeping rows: `serve` registers the hosted folder
    // against our own id, which is never a sync target. When a real remote
    // is attached, re-point those rows instead of duplicating them.
    for (id, path, dev, _dir, _last) in storage.list_sync_folders()? {
        if path == folder && dev == own_device_id {
            storage.set_folder_device(id, &row_device)?;
            println!("Attached '{path}' → {device}");
        }
    }
    let folder_id = get_or_create_folder(&storage, &folder, &row_device)?;
    let Some(addr) = resolved else {
        bail!("{row_device} has no recorded address yet — run 'discover', or have it pair again");
    };
    println!("Syncing {folder} with {addr}...");
    let state_store = Arc::new(InMemoryStateStore::new());
    let engine = SyncEngine::new(storage, crypto, ferrisync_core::DeviceInfo {
        id: own_device_id.to_string(),
        name: String::new(),
        cert_fingerprint: Vec::new(),
    }, state_store);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait_secs);
    let mut waiting = false;
    loop {
        match engine.run_sync(&folder, addr, folder_id, &row_device).await {
            Ok(result) => {
                println!(
                    "Sync complete. Pushed: {}, Pulled: {}",
                    result.pushed.len(),
                    result.pulled.len(),
                );
                return Ok(());
            }
            Err(e)
                if wait_secs > 0
                    && e.to_string().contains("could not reach")
                    && std::time::Instant::now() < deadline =>
            {
                if !waiting {
                    println!("Waiting up to {wait_secs}s for the peer to become reachable…");
                    waiting = true;
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Sync every configured sync folder against its known device address.
pub async fn run_all(
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    own_device_id: &str,
) -> anyhow::Result<()> {
    let state_store = Arc::new(InMemoryStateStore::new());
    let engine = SyncEngine::new(storage, crypto, ferrisync_core::DeviceInfo {
        id: own_device_id.to_string(),
        name: String::new(),
        cert_fingerprint: Vec::new(),
    }, state_store);
    let outcomes = engine.sync_all_folders().await?;
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
    wait_secs: u64,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    own_device_id: &str,
) -> anyhow::Result<()> {
    match (folder, device) {
        (Some(folder), Some(device)) => {
            run(folder, device, storage, crypto, own_device_id, wait_secs).await
        }
        (None, None) => run_all(storage, crypto, own_device_id).await,
        _ => anyhow::bail!("usage: sync [<folder> --device <ip[:port]|name|uuid> [--wait secs]]"),
    }
}

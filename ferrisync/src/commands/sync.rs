use anyhow::bail;

use crate::app::ApplicationContext;

use super::args::SyncArgs;
use super::device::ensure_device;
use super::fmt;
use super::resolve_device_key;
use super::watch::get_or_create_folder;

/// One-shot folder sync. With no folder/device, sync every configured
/// folder. Shared by the CLI subcommand and the REPL.
pub async fn run(ctx: &ApplicationContext, args: &SyncArgs) -> anyhow::Result<()> {
    match (&args.folder, &args.device) {
        (Some(folder), Some(device)) => run_single(ctx, folder, device, args.wait).await,
        (None, None) => run_all(ctx).await,
        _ => bail!("usage: sync [<folder> --device <ip[:port]|name|uuid> [--wait secs]]"),
    }
}

async fn run_single(
    ctx: &ApplicationContext,
    folder: &str,
    device: &str,
    wait_secs: u64,
) -> anyhow::Result<()> {
    let (row_device, resolved) = resolve_device_key(&ctx.storage, device, &ctx.device_info.id)?;
    if row_device == device {
        // Legacy ip-keyed row: make sure the device exists for the FK.
        ensure_device(&ctx.storage, &row_device)?;
    }
    // Adopt served-bookkeeping rows: `serve` registers the hosted folder
    // against our own id, which is never a sync target. When a real remote
    // is attached, re-point those rows instead of duplicating them.
    for (id, path, dev, _dir, _last) in ctx.storage.list_sync_folders()? {
        if path == folder && dev == ctx.device_info.id {
            ctx.storage.set_folder_device(id, &row_device)?;
            println!("Attached '{path}' → {device}");
        }
    }
    let folder_id = get_or_create_folder(&ctx.storage, folder, &row_device)?;
    let Some(addr) = resolved else {
        anyhow::bail!(
            "{device} is not reachable yet — run `ferrisync devices pair <ip>`, \
             or open FerriSync on it so its address is recorded"
        );
    };
    println!("Syncing {folder} with {addr}...");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait_secs);
    let mut waiting = false;
    loop {
        match ctx
            .engine
            .run_sync(folder, addr, folder_id, &row_device)
            .await
        {
            Ok(result) => {
                println!(
                    "Sync complete. Pushed: {} file(s), Pulled: {} file(s), Conflicts: {}",
                    result.pushed.len(),
                    result.pulled.len(),
                    result.conflicts.len(),
                );
                if !result.conflicts.is_empty() {
                    println!(
                        "  {n} conflict(s) to review — run `ferrisync conflicts`",
                        n = result.conflicts.len()
                    );
                }
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
async fn run_all(ctx: &ApplicationContext) -> anyhow::Result<()> {
    let outcomes = ctx.engine.sync_all_folders().await?;
    if outcomes.is_empty() {
        println!("No sync folders configured.");
        return Ok(());
    }

    let mut synced = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut bytes_total: u64 = 0;
    let mut conflicts_total = 0usize;
    for outcome in &outcomes {
        match (&outcome.addr, &outcome.result) {
            (None, Some(Err(e))) => {
                skipped += 1;
                println!("Note {}: {}", outcome.path, fmt::friendly_error(e));
            }
            (None, _) => {
                skipped += 1;
                println!(
                    "Skipped {} — no known address for {}; pair or discover first.",
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
                bytes_total += result.pulled_bytes + result.pushed_bytes;
                conflicts_total += result.conflicts.len();
                println!(
                    "Synced {path} with {addr}. Pushed: {}, Pulled: {}, Conflicts: {}{loopback_hint}",
                    result.pushed.len(),
                    result.pulled.len(),
                    result.conflicts.len(),
                    path = outcome.path,
                );
            }
            (Some(_), Some(Err(e))) => {
                failed += 1;
                println!(
                    "Failed to sync {}: {}",
                    outcome.path,
                    fmt::friendly_error(e)
                );
            }
            (Some(_), None) => unreachable!("session ran but produced no result"),
        }
    }

    let summary = format!(
        "Done: {synced} synced, {failed} failed, {skipped} skipped. {conflicts_total} conflict(s), {bytes} transferred.",
        bytes = crate::commands::fmt::bytes_human(bytes_total as f64),
    );
    if synced == 0 && failed + skipped > 0 {
        bail!("{summary}");
    }
    println!("{summary}");
    Ok(())
}

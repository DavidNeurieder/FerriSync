use anyhow::{bail, Context};
use ferrisync_core::health::{self, FolderHealth};

use crate::app::ApplicationContext;
use crate::commands::device::resolve_device_id;
use crate::commands::input::read_yes_no;
use crate::commands::watch::get_or_create_folder;

use super::fmt;

/// `ferrisync folders` — list every sync folder with its derived health.
pub fn list(ctx: &ApplicationContext, json: bool) -> anyhow::Result<()> {
    let folders = health::compute_folder_statuses(
        &ctx.storage,
        &ctx.device_info.id,
        health::now_secs(),
        &health::LiveState::default(),
    )?;

    if json {
        let out = serde_json::to_string_pretty(&folders)?;
        println!("{out}");
        return Ok(());
    }

    if folders.is_empty() {
        println!("No sync folders configured.");
        println!("  Add one: ferrisync folders add <path> --device <name|id>");
        return Ok(());
    }

    for f in &folders {
        let peer = f.peer_label(&ctx.device_info.id);
        println!(
            "  {:<40} ↔ {:<20} {:<14} last sync: {:<10} {} conflict(s)",
            f.path,
            peer,
            health_label(f.health),
            fmt::relative(f.last_sync),
            f.conflicts,
        );
    }
    Ok(())
}

/// `ferrisync folders add <path> --device <name|id>` — configure a folder
/// against a paired device.
pub async fn add(ctx: &ApplicationContext, path: &str, device: &str) -> anyhow::Result<()> {
    if !std::path::Path::new(path).is_dir() {
        bail!("'{path}' is not a directory");
    }
    let (row_device, device_name) =
        resolve_device_id(&ctx.storage, device, &ctx.device_info.id)?;

    println!(
        "Sync '{path}' with {device_name}? [y/N] "
    );
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    if !read_yes_no().await {
        println!("Aborted.");
        return Ok(());
    }

    // Adopt served-bookkeeping rows pointing at ourselves, then create/find
    // the real row.
    for (id, p, dev, _dir, _last) in ctx.storage.list_sync_folders()? {
        if p == path && dev == ctx.device_info.id {
            ctx.storage.set_folder_device(id, &row_device)?;
            println!("Attached '{path}' → {device_name}");
        }
    }
    let folder_id = get_or_create_folder(&ctx.storage, path, &row_device)?;
    println!(
        "Syncing '{path}' with '{device_name}' (folder id {folder_id}).",
    );
    Ok(())
}

/// `ferrisync folders remove <path> [--device <name|id>]` — forget a configured
/// folder (deletes all sync metadata/history for it).
pub async fn remove(ctx: &ApplicationContext, path: &str, device: Option<&str>, yes: bool) -> anyhow::Result<()> {
    let device_id = match device {
        Some(dev) => Some(resolve_device_id(&ctx.storage, dev, &ctx.device_info.id)?.0),
        None => None,
    };

    if !yes {
        println!(
            "Remove sync entry for '{path}'{}?",
            device.map(|d| format!(" with {d}")).unwrap_or_default()
        );
        print!("Continue? [y/N] ");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        if !read_yes_no().await {
            println!("Aborted.");
            return Ok(());
        }
    }

    let removed = ctx
        .storage
        .remove_sync_folders(path, device_id.as_deref())
        .context("remove sync folder")?;
    if removed == 0 {
        println!("no sync entry for '{path}'");
    } else {
        println!(
            "Removed {removed} sync entr{} for '{path}'.",
            if removed == 1 { "y" } else { "ies" }
        );
    }
    Ok(())
}

fn health_label(h: FolderHealth) -> &'static str {
    match h {
        FolderHealth::Healthy => "healthy",
        FolderHealth::Syncing => "syncing",
        FolderHealth::Waiting => "waiting",
        FolderHealth::Offline => "offline",
        FolderHealth::Error => "error",
        FolderHealth::Conflict => "conflict",
        FolderHealth::NotConfigured => "not configured",
    }
}
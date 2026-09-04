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
    // Show one row per physical folder, not one per (folder, device) pair.
    let folders = health::group_folders(&folders, &ctx.device_info.id);

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

    let remote = ctx.storage.folder_remote_labels(&ctx.device_info.id)?;
    for f in &folders {
        // Show the remote folder path on the peer, not just the peer name.
        let peer = remote
            .get(&f.id)
            .cloned()
            .unwrap_or_else(|| f.peer_label(&ctx.device_info.id));
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
    let (row_device, device_name) = resolve_device_id(&ctx.storage, device, &ctx.device_info.id)?;

    println!("Sync '{path}' with {device_name}? [y/N] ");
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
    get_or_create_folder(&ctx.storage, path, &row_device)?;
    println!("Syncing '{path}' with '{device_name}'.");
    Ok(())
}

/// `ferrisync folders remove <path> [--device <name|id>]` — forget a configured
/// folder (deletes all sync metadata/history for it).
pub async fn remove(
    ctx: &ApplicationContext,
    path: &str,
    device: Option<&str>,
    yes: bool,
) -> anyhow::Result<()> {
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

fn folder_id_for_path(ctx: &ApplicationContext, path: &str) -> anyhow::Result<i64> {
    for (id, p, _dev, _dir, _last) in ctx.storage.list_sync_folders()? {
        if p == path {
            return Ok(id);
        }
    }
    anyhow::bail!("no sync folder for '{path}' — add one with `ferrisync folders add`")
}

/// `ferrisync folders status <path>` — every device this folder syncs with,
/// plus each pair's mode and remote path.
pub fn status(ctx: &ApplicationContext, path: &str) -> anyhow::Result<()> {
    let folder_id = folder_id_for_path(ctx, path)?;
    let pairs = ctx.storage.folder_pairs(folder_id)?;

    println!("{path}");
    println!("  local path:  {path}");
    if pairs.is_empty() {
        println!("  (not syncing with any device — use `folders add-device`)");
        return Ok(());
    }

    for (device_id, mode, remote_path, enabled) in pairs {
        let name = ctx
            .storage
            .list_devices()?
            .into_iter()
            .find(|(id, _, _)| id == &device_id)
            .map(|(_, n, _)| n)
            .unwrap_or_else(|| device_id.clone());
        let state = if enabled { "" } else { " (disabled)" };
        println!(
            "  ↔ {:<20} {:>11}  remote: {}{}",
            name,
            mode,
            remote_path.as_deref().unwrap_or(path),
            state,
        );
    }
    Ok(())
}

/// `ferrisync folders add-device <path> --device <name|id> [--remote-path P]`
/// — attach one more paired device to an existing folder. Reuses the folder's
/// config rather than creating a new folder row (many-to-many graph).
pub async fn add_device(
    ctx: &ApplicationContext,
    path: &str,
    device: &str,
    remote_path: Option<&str>,
    mode: &str,
) -> anyhow::Result<()> {
    if !matches!(mode, "bidirectional" | "send-only" | "receive-only") {
        anyhow::bail!("invalid mode {mode:?} — use bidirectional, send-only or receive-only");
    }
    if !std::path::Path::new(path).is_dir() {
        anyhow::bail!("'{path}' is not a directory");
    }
    let folder_id = folder_id_for_path(ctx, path)?;
    let (row_device, device_name) = resolve_device_id(&ctx.storage, device, &ctx.device_info.id)?;

    let existing: Vec<String> = ctx
        .storage
        .folder_pairs(folder_id)?
        .into_iter()
        .map(|(d, _, _, _)| d)
        .collect();
    if existing.contains(&row_device) {
        anyhow::bail!("'{path}' already syncs with {device_name}");
    }

    let resolved_remote = remote_path.map(|p| p.to_string());
    ctx.storage
        .add_folder_device(folder_id, &row_device, mode, resolved_remote.as_deref())?;
    println!(
        "Syncing '{path}' → {device_name} (mode {mode}, remote: {})",
        resolved_remote.as_deref().unwrap_or(path),
    );
    Ok(())
}

/// `ferrisync folders remove-device <path> --device <name|id>` — drop a single
/// folder↔device pairing. Never deletes files.
pub async fn remove_device(
    ctx: &ApplicationContext,
    path: &str,
    device: &str,
    yes: bool,
) -> anyhow::Result<()> {
    let folder_id = folder_id_for_path(ctx, path)?;
    let (row_device, device_name) = resolve_device_id(&ctx.storage, device, &ctx.device_info.id)?;

    if !yes {
        println!("Stop syncing '{path}' with {device_name}? (files are kept)");
        print!("Continue? [y/N] ");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        if !read_yes_no().await {
            println!("Aborted.");
            return Ok(());
        }
    }

    let removed = ctx.storage.remove_folder_device(folder_id, &row_device)?;
    if removed {
        println!("Stopped syncing '{path}' with {device_name} (files kept).");
    } else {
        println!("'{path}' is not syncing with {device_name}.");
    }
    Ok(())
}

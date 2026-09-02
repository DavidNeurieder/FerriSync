//! `ferrisync share` — publish and manage folders other devices can discover
//! and request to pair to. These are one-shot storage operations (no live
//! server needed); the peer-pairing side lives in `folders browse/request`.

use anyhow::Result;
use ferrisync_core::storage::{path_label, SharedFolderRow};

use crate::app::ApplicationContext;

/// `ferrisync share list` — every shared folder this device publishes.
pub fn list(ctx: &ApplicationContext, json: bool) -> Result<()> {
    let own = ctx.device_info.id.clone();
    let rows = ctx.storage.list_shared_folders(&own)?;
    if json {
        let simple: Vec<serde_json::Value> = rows.iter().map(share_json).collect();
        println!("{}", serde_json::to_string_pretty(&simple)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("No shared folders.");
        println!("  Publish one: ferrisync share add <path> [--name <name>]");
        return Ok(());
    }
    println!("id  guid        name           local_path      visible");
    for r in &rows {
        println!(
            "{:<3} {:<12} {:<14} {:<15} {}",
            r.0,
            &r.1[..r.1.len().min(12)],
            r.3,
            r.4,
            if r.5 { "yes" } else { "no" }
        );
    }
    Ok(())
}

/// `ferrisync share add <path>` — publish a local folder as a discoverable
/// shared folder. Idempotent; a folder already shared stays shared.
pub fn add(ctx: &ApplicationContext, path: &str, name: Option<&str>) -> Result<()> {
    let own = ctx.device_info.id.clone();
    let all = ctx.storage.list_shared_folders(&own)?;
    if let Some(existing) = all.iter().find(|r| r.4 == path) {
        println!(
            "Already shared (share id {}): {}",
            existing.0, path
        );
        return Ok(());
    }
    let display = name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path_label(path));
    // The share row's device_id references `devices.id`; ensure the owner is
    // registered first (the app does this when serving, but a bare `share add`
    // may run before any pairing/serve).
    ctx.storage
        .upsert_device(&own, &ctx.device_info.name, None, None)?;
    // Sharing is keyed to the folder's stable guid. Ensure a sync_folders row
    // exists for this path (reusing any existing one) so we share a real,
    // migration-compatible guid rather than a loose label.
    let existing_id = ctx
        .storage
        .list_sync_folders()?
        .into_iter()
        .find(|(_id, p, _d, _dir, _l)| p == path)
        .map(|(id, _p, _d, _dir, _l)| id);
    let folder_id = match existing_id {
        Some(id) => id,
        None => ctx.storage.add_sync_folder(path, &own, "bidirectional")?,
    };
    let guid = ctx
        .storage
        .folder_guid(folder_id)?
        .unwrap_or_else(|| ferrisync_core::storage::new_folder_guid(folder_id));
    ctx.storage
        .share_folder(&guid, &own, &display, path)?;
    let row = ctx
        .storage
        .shared_folder_by_guid(&own, &guid)?
        .expect("share persisted");
    println!("Shared folder id {}: {} ({})", row.0, display, path);
    Ok(())
}

/// `ferrisync share discover <share-id> [--enabled true|false]` — toggle
/// whether the share is visible to trusted peers browsing the LAN.
pub fn discover(ctx: &ApplicationContext, share_id: i64, enabled: bool) -> Result<()> {
    ctx.storage.set_shared_discoverable(share_id, enabled)?;
    println!(
        "Share {share_id} is now {}.",
        if enabled { "discoverable" } else { "hidden" }
    );
    Ok(())
}

/// `ferrisync share off <share-id>` — stop sharing; existing peer pairs kept.
pub fn off(ctx: &ApplicationContext, share_id: i64) -> Result<()> {
    ctx.storage.unshare_folder(share_id)?;
    println!("Stopped sharing {share_id}.");
    Ok(())
}

fn share_json(r: &SharedFolderRow) -> serde_json::Value {
    serde_json::json!({
        "id": r.0,
        "folder_guid": r.1,
        "name": r.3,
        "local_path": r.4,
        "discoverable": r.5,
        "enabled": r.6,
        "permissions": r.7,
    })
}
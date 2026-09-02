//! `ferrisync folders browse/request` — the requester side of shared-folder
//! pairing. Runs over TLS against an already-paired device (post-trust), so it
//! needs no live server on this end.
//!
//! `folders approve/deny` are handled by the interactive `serve` session (the
//! only place a live approval gate exists); this module documents that and
//! registers the approved replica when possible.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use ferrisync_core::sync_engine::shared_folder::{FolderPairReply, SharedFolderClient};

use crate::app::ApplicationContext;

/// `ferrisync folders browse <ip> [--port P]` — list a paired device's
/// discoverable shared folders.
pub async fn browse(ctx: &ApplicationContext, ip: &str, port: u16) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{ip}:{port}").parse()?;
    let client = SharedFolderClient::new(ctx.crypto.clone(), addr);
    let folders = client.list_shared_folders().await?;
    if folders.is_empty() {
        println!("{ip} has no discoverable shared folders.");
        return Ok(());
    }
    println!("Folders shared by {ip}:");
    for f in &folders {
        println!("  {:<12} {}  [{}]", f.folder_guid, f.name, f.mode);
    }
    println!();
    println!("  Request one: ferrisync folders request {ip} <guid> --path <dir>");
    Ok(())
}

/// `ferrisync folders request <ip> <guid> --path <dir> [--name N] [--seconds S]`
///
/// Ask the paired device to pair us to one of its shared folders, and keep
/// polling until the owner approves (or `seconds` elapses). On approval this
/// registers a local replica of the logical folder so it starts syncing.
pub async fn request(
    ctx: &ApplicationContext,
    ip: &str,
    port: u16,
    guid: &str,
    local_path: &str,
    name: Option<&str>,
    seconds: u64,
) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{ip}:{port}").parse()?;
    let client = SharedFolderClient::new(ctx.crypto.clone(), addr);
    let display = name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| guid.to_string());
    println!("Requesting pairing to {guid} on {ip}:{port} …");

    let reply = client
        .request_and_collect_pairing(
            &ctx.device_info.id,
            &ctx.device_info.name,
            guid,
            &display,
            Some(Duration::from_secs(seconds.max(1))),
        )
        .await
        .context("folder pairing request failed")?;

    match reply {
        FolderPairReply::Approved(grant) => {
            register_replica(ctx, guid, &display, local_path, &grant.remote_path)?;
            println!(
                "Approved by {ip}. Folder '{}' is now paired; syncing at {local_path}.",
                grant.name
            );
        }
        FolderPairReply::Rejected(reason) => {
            println!("The owner rejected the pairing: {reason}");
        }
        FolderPairReply::Pending => {
            println!(
                "Still waiting for the owner's approval. Run again (or wait) to re-poll."
            );
        }
    }
    Ok(())
}

/// `ferrisync folders approve <device> <guid>` — approval happens interactively
/// in a serve session; this helper reports how to approve there.
pub fn approve(ctx: &ApplicationContext, _device: &str, _guid: &str) -> anyhow::Result<()> {
    let _ = ctx;
    bail_serve_only("approve")
}

/// `ferrisync folders deny <device> <guid>` — denial happens interactively in
/// a serve session.
pub fn deny(ctx: &ApplicationContext, _device: &str, _guid: &str) -> anyhow::Result<()> {
    let _ = ctx;
    bail_serve_only("deny")
}

fn bail_serve_only(action: &str) -> anyhow::Result<()> {
    anyhow::bail!(
        "folder {action} happens interactively while `ferrisync serve` is running \
         (it prompts when a peer requests one of your shared folders)"
    )
}

/// Persist a local replica of the approved logical folder on this (requester)
/// device, reusing the owner's guid so both sides track the same folder.
fn register_replica(
    ctx: &ApplicationContext,
    guid: &str,
    display: &str,
    local_path: &str,
    remote_path: &Option<String>,
) -> anyhow::Result<()> {
    let own = ctx.device_info.id.clone();
    let folder_id = ctx
        .storage
        .ensure_folder_by_guid(guid, local_path, display, &own)?;
    let peer_path = remote_path
        .clone()
        .unwrap_or_else(|| local_path.to_string());
    // The peer id is the address we paired to via the granted pairing; we
    // stored the owner's share locally under `peer`. We don't know the owner's
    // device id from the grant alone, so record it by the last-known name.
    let peer_id = resolve_owner_id(ctx, display)?;
    ctx.storage
        .add_folder_device(folder_id, &peer_id, "bidirectional", Some(&peer_path))?;
    Ok(())
}

/// Resolve the owner device id for the paired peer, preferring an exact id or
/// name match over the generic last-seen peer. Falls back to the device name
/// as the pair key when unknown.
fn resolve_owner_id(ctx: &ApplicationContext, name_or_id: &str) -> anyhow::Result<String> {
    let devices = ctx.storage.list_devices()?;
    if let Some((id, _, _)) = devices.iter().find(|(id, _, _)| id == name_or_id) {
        return Ok(id.clone());
    }
    if let Some((id, _, _)) = devices.iter().find(|(_, name, _)| name == name_or_id) {
        return Ok(id.clone());
    }
    // Last resort: address the pair by name so approval still wires a replica.
    Ok(name_or_id.to_string())
}
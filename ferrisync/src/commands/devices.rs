use anyhow::Context;
use ferrisync_core::discovery::{DiscoveredPeer, DiscoveryService};
use ferrisync_core::health::{self, Presence};
use std::time::Duration;

use crate::app::ApplicationContext;
use crate::commands::device::{parse_device, resolve_device_id};
use crate::commands::remove as remove_op;

/// `ferrisync devices` — list paired devices with their presence (default).
pub fn list(ctx: &ApplicationContext, json: bool) -> anyhow::Result<()> {
    let statuses =
        health::compute_device_statuses(&ctx.storage, &ctx.device_info.id, health::now_secs())?;

    if json {
        let out = serde_json::to_string_pretty(&statuses)?;
        println!("{out}");
        return Ok(());
    }

    if statuses.is_empty() {
        println!("No paired devices.");
        println!("  Discover nearby devices:  ferrisync devices discover");
        println!("  Pair:                     ferrisync devices pair");
        return Ok(());
    }

    for d in &statuses {
        let last = presence_label(d.presence);
        let folders = if d.folder_count == 1 {
            "1 folder".to_string()
        } else {
            format!("{} folders", d.folder_count)
        };
        println!("  {:<24} {:<14} {}", d.name, last, folders);
    }
    Ok(())
}

/// `ferrisync devices discover [--seconds N]` — one-shot LAN scan.
pub async fn discover(ctx: &ApplicationContext, seconds: u32) -> anyhow::Result<()> {
    // Flag a second instance announcing under the same name before the scan.
    if let Some(warning) = ferrisync_core::discovery::duplicate_announce_warning(
        &ctx.device_info,
        crate::commands::DEFAULT_PORT,
    ) {
        println!("{warning}");
    }

    println!("Scanning the LAN for FerriSync devices ({seconds}s)...");
    let peers = scan(ctx, seconds).await;
    if peers.is_empty() {
        println!("(no devices found)");
    } else {
        for (i, peer) in peers.iter().enumerate() {
            println!("  {}. {}", i + 1, peer.name);
        }
    }
    Ok(())
}

/// `ferrisync devices pair [<ip>]` — pair with a known address, or run an
/// interactive discovery when no address is given.
pub async fn pair(ctx: &ApplicationContext, ip: Option<String>, port: u16) -> anyhow::Result<()> {
    let addr = match ip {
        Some(ip) => parse_device(&ip, port)?,
        None => {
            let peers = scan(ctx, 4).await;
            if peers.is_empty() {
                anyhow::bail!(
                    "no devices found on the LAN — run `ferrisync devices pair <ip>` to reach one directly"
                );
            }
            println!("Devices found:");
            for (i, peer) in peers.iter().enumerate() {
                println!("  {}. {}", i + 1, peer.name);
            }
            print!("Pair with device # [1-{}] (Enter to cancel): ", peers.len());
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            let idx: Result<usize, _> = line.trim().parse();
            let idx: usize = match idx {
                Ok(n) if n >= 1 && n <= peers.len() => n - 1,
                _ => {
                    println!("Aborted.");
                    return Ok(());
                }
            };
            peers[idx]
                .addresses
                .first()
                .copied()
                .context("discovered device has no usable address")?
        }
    };

    let peer = ctx
        .pairing
        .pair_with(addr)
        .await
        .context("Pairing failed")?;
    println!("Paired with {}", peer.name);
    Ok(())
}

/// `ferrisync devices rename <device> <name>` — rename a paired device by
/// name or id.
pub fn rename(ctx: &ApplicationContext, device: &str, name: &str) -> anyhow::Result<()> {
    let clean = ferrisync_core::api::sanitize_device_name(name)?;
    let (id, current) = resolve_device_id(&ctx.storage, device, &ctx.device_info.id)?;
    ctx.storage.upsert_device(&id, &clean, None, None)?;
    println!("Renamed '{current}' → '{clean}'.");
    Ok(())
}

/// `ferrisync devices remove <device>` — remove a paired device by name or id
/// after confirmation.
pub async fn remove(ctx: &ApplicationContext, device: &str, yes: bool) -> anyhow::Result<()> {
    let (id, _name) = resolve_device_id(&ctx.storage, device, &ctx.device_info.id)?;
    remove_op::run(ctx, &id, yes).await
}

/// One coarse-grained LAN scan, deduplicated by device id.
async fn scan(ctx: &ApplicationContext, seconds: u32) -> Vec<DiscoveredPeer> {
    let service =
        match DiscoveryService::new(ctx.device_info.clone(), crate::commands::DEFAULT_PORT) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: mDNS discovery failed: {e}");
                return Vec::new();
            }
        };
    let Ok(mut rx) = service.browse() else {
        eprintln!("error: mDNS browse failed");
        return Vec::new();
    };

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds as u64);
    let mut peers: Vec<DiscoveredPeer> = Vec::new();
    while let Ok(Some(peer)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        if peer.id == ctx.device_info.id {
            continue;
        }
        if !peers.iter().any(|p| p.id == peer.id) {
            peers.push(peer);
        }
    }
    service.shutdown();
    peers
}

fn presence_label(p: Presence) -> &'static str {
    match p {
        Presence::Connected => "connected",
        Presence::RecentlySeen => "recently seen",
        Presence::Offline => "offline",
    }
}

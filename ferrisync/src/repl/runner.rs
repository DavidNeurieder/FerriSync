use anyhow::Result;
use ferrisync_core::discovery::{DiscoveredPeer, DiscoveryService};
use ferrisync_core::DeviceInfo;
use std::time::Duration;

use crate::app::ApplicationContext;
use crate::commands::status as status_op;
use crate::commands::{pair as pair_op, sync as sync_op};

use super::commands::ReplCommand;
use super::state::ReplState;

pub async fn dispatch(state: &mut ReplState, ctx: &ApplicationContext, command: ReplCommand) {
    match command {
        ReplCommand::Status => {
            let result = status_op::run(ctx).map(|status| {
                print!("{}", status_op::format(&status));
            });
            handle(result);
        }
        ReplCommand::Sessions => match ctx.storage.list_recent_sessions(15) {
            Ok(rows) if rows.is_empty() => println!("No sessions recorded yet."),
            Ok(rows) => {
                for r in rows {
                    let when = chrono::DateTime::from_timestamp(r.ts, 0)
                        .map(|dt| dt.format("%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| r.ts.to_string());
                    println!(
                        "[{}] {} {} @ {} — pushed {}, pulled {}, conflicts {} ({})",
                        when,
                        r.direction,
                        r.peer_device,
                        r.addr,
                        r.pushed_count,
                        r.pulled_count,
                        r.conflicts_count,
                        r.folder_path,
                    );
                }
            }
            Err(e) => eprintln!("error: {e:#}"),
        },
        ReplCommand::Discover { seconds } => discover(&state.device_info, seconds).await,
        ReplCommand::Pair { ip, port } => handle(pair_op::run(ctx, &ip, port).await),
        ReplCommand::Sync(args) => handle(sync_op::run(ctx, &args).await),
        ReplCommand::Unsync {
            folder,
            device,
            yes,
        } => unsync(state, ctx, folder, device, yes),
        ReplCommand::Watch(args) => {
            state.start_watch(&ctx.storage, &ctx.engine, args.folder, args.device);
        }
        ReplCommand::Watches => state.list_watches(),
        ReplCommand::Unwatch { id } => state.stop_watch(id).await,
        ReplCommand::Serve { folder, port } => state.start_server(ctx, folder, port).await,
        ReplCommand::Serves => state.list_servers(),
        ReplCommand::Unserve { id } => state.stop_server(id).await,
        ReplCommand::Pendings => state.list_pendings(),
        ReplCommand::Confirm { n } => state.resolve_pending(n, true),
        ReplCommand::Deny { n } => state.resolve_pending(n, false),
        ReplCommand::Yes => state.answer_latest(true),
        ReplCommand::No => state.answer_latest(false),
        ReplCommand::Rename { name } => rename(state, ctx, &name).await,
        ReplCommand::Help | ReplCommand::Exit => unreachable!("help/exit are handled by the loop"),
    }
}

fn handle(result: Result<()>) {
    if let Err(e) = result {
        eprintln!("error: {e:#}");
    }
}

fn unsync(
    state: &ReplState,
    ctx: &ApplicationContext,
    folder: Option<String>,
    device: Option<String>,
    yes: bool,
) {
    match (folder, device, yes) {
        (None, None, false) => {
            let folders = ctx.storage.list_sync_folders().map(|v| v.len()).unwrap_or(0);
            let devices = ctx.storage.list_devices().map(|v| v.len()).unwrap_or(0);
            if folders == 0 && devices == 0 {
                println!("nothing to clear: no folders or devices known");
            } else {
                println!(
                    "this would remove {folders} folder entr{} and {devices} device{}; run `unsync --yes` to confirm",
                    if folders == 1 { "y" } else { "ies" },
                    if devices == 1 { "" } else { "s" },
                );
            }
        }
        (None, None, true) => match ctx.storage.clear_all_sync_state() {
            Ok((f, d)) => {
                println!(
                    "Removed {f} folder entr{} and {d} device{} (metadata cleared).",
                    if f == 1 { "y" } else { "ies" },
                    if d == 1 { "" } else { "s" }
                );
                if state.has_background() {
                    println!(
                        "note: background watches/serves are still running; stop them with 'unwatch'/'unserve'"
                    );
                }
            }
            Err(e) => eprintln!("error: {e:#}"),
        },
        (folder, device, false) => {
            let Some(folder) = folder else {
                unreachable!("scoped unsync requires a folder");
            };
            match ctx.storage.remove_sync_folders(&folder, device.as_deref()) {
                Ok(0) => println!("no sync entries for '{folder}'"),
                Ok(n) => println!(
                    "removed {n} sync entr{} for '{folder}'",
                    if n == 1 { "y" } else { "ies" }
                ),
                Err(e) => eprintln!("error: {e:#}"),
            }
        }
        (_, _, true) => unreachable!("--yes conflicts rejected at parse time"),
    }
}

async fn rename(state: &mut ReplState, ctx: &ApplicationContext, name: &str) {
    match ferrisync_core::api::sanitize_device_name(name) {
        Ok(clean) => {
            ferrisync_core::config::persist_device_name(&ctx.data_dir, &clean);
            state.device_info.name = clean.clone();
            ctx.pairing.set_name(&clean);
            state.rename_restart_servers(ctx).await;
            println!("Renamed to '{clean}'.");
        }
        Err(e) => eprintln!("error: {e:#}"),
    }
}

async fn discover(device_info: &DeviceInfo, seconds: u64) {
    println!("Scanning the LAN for FerriSync devices ({seconds}s)...");
    let service = match DiscoveryService::new(device_info.clone(), crate::commands::DEFAULT_PORT) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: mDNS discovery failed: {e}");
            return;
        }
    };

    let mut rx = match service.browse() {
        Ok(rx) => rx,
        Err(e) => {
            eprintln!("error: mDNS browse failed: {e}");
            return;
        }
    };

    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut peers: Vec<DiscoveredPeer> = Vec::new();
    while let Ok(Some(peer)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        // Never list ourselves; a stale self-row is how self-syncs happen.
        if peer.id == device_info.id {
            continue;
        }
        if !peers.iter().any(|p| p.id == peer.id) {
            peers.push(peer);
        }
    }
    service.shutdown();

    if peers.is_empty() {
        println!("(no devices found)");
        return;
    }
    for peer in peers {
        let addrs: Vec<String> = peer.addresses.iter().map(|a| a.to_string()).collect();
        println!("  {}  [{}]", peer.name, addrs.join(", "));
    }
}
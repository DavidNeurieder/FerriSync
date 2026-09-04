use anyhow::Result;
use ferrisync_core::discovery::{DiscoveredPeer, DiscoveryService};
use ferrisync_core::DeviceInfo;
use std::time::Duration;

use crate::app::ApplicationContext;
use crate::commands::status as status_op;
use crate::commands::{
    activity as activity_op, add as add_op, conflicts as conflicts_op, devices as devices_op,
    doctor as doctor_op, folders as folders_op, pair as pair_op, sync as sync_op,
};

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
        ReplCommand::Devices => handle(devices_op::list(ctx, false)),
        ReplCommand::Folders => handle(folders_op::list(ctx, false)),
        ReplCommand::Activity => handle(activity_op::run(ctx, 15, false)),
        ReplCommand::Conflicts => handle(conflicts_op::list(ctx, None)),
        ReplCommand::Doctor => {
            let result = doctor_op::run(ctx, false).await.map(|_| ());
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
        ReplCommand::Add { path, name } => handle(add_op::run(ctx, &path, name.as_deref())),
        ReplCommand::Sync(args) => match sync_op::run(ctx, &args).await {
            Ok(()) => {}
            Err(e) => {
                let e = match args.device.as_deref() {
                    Some(q) => with_last_seen(ctx, q, e),
                    None => e,
                };
                eprintln!("error: {}", crate::commands::fmt::friendly_error(&e));
            }
        },
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
        ReplCommand::Serve { folder, port } => match folder {
            Some(folder) => state.start_server(ctx, folder, port).await,
            None => state.start_all_servers(ctx, port).await,
        },
        ReplCommand::Serves => state.list_servers(),
        ReplCommand::Unserve { id } => state.stop_server(id).await,
        ReplCommand::Pendings => state.list_pendings(),
        ReplCommand::Confirm { n } => state.resolve_pending(n, true),
        ReplCommand::Deny { n } => state.resolve_pending(n, false),
        ReplCommand::Yes => state.answer_latest(true),
        ReplCommand::No => state.answer_latest(false),
        ReplCommand::Rename { name } => rename(state, ctx, &name).await,
        ReplCommand::Reset { yes } => reset(state, ctx, yes).await,
        ReplCommand::Help | ReplCommand::Exit => unreachable!("help/exit are handled by the loop"),
    }
}

fn handle(result: Result<()>) {
    if let Err(e) = result {
        eprintln!("error: {}", crate::commands::fmt::friendly_error(&e));
    }
}

/// Enrich an unreachable-peer error with a "was last seen X ago" hint when the
/// failing target can be matched to a stored device (by name, uuid, or id).
fn with_last_seen(ctx: &ApplicationContext, query: &str, err: anyhow::Error) -> anyhow::Error {
    let friendly = crate::commands::fmt::friendly_error(&err);
    let lower = friendly.to_lowercase();
    let is_unreachable = lower.contains("could not reach")
        || lower.contains("timed out")
        || lower.contains("refused");
    if !is_unreachable {
        return err;
    }
    let matched = match ctx.storage.list_devices() {
        Ok(devices) => last_seen_hint(&devices, query),
        Err(_) => None,
    };
    match matched {
        Some(hint) => anyhow::anyhow!("{friendly} — {hint}"),
        None => err,
    }
}

/// Match a device (by name, id, or contained id) against stored `(id, name,
/// last_seen)` rows and produce a "was last seen X ago" hint for it.
fn last_seen_hint(devices: &[(String, String, Option<i64>)], query: &str) -> Option<String> {
    devices
        .iter()
        .find(|(id, name, _last_seen)| {
            name.eq_ignore_ascii_case(query) || id.eq_ignore_ascii_case(query) || query.contains(id)
        })
        .map(|(_id, name, last_seen)| {
            format!(
                "{name} was last seen {}",
                crate::commands::fmt::relative(*last_seen)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::last_seen_hint;

    #[test]
    fn matches_by_name_case_insensitively() {
        let devices = vec![
            ("uuid-1".to_string(), "Pixel 9".to_string(), Some(1000)),
            ("uuid-2".to_string(), "Desktop".to_string(), None),
        ];
        let hint = last_seen_hint(&devices, "pixel 9").unwrap();
        assert!(hint.contains("Pixel 9"), "{hint}");
        assert!(hint.contains("was last seen"), "{hint}");
    }

    #[test]
    fn matches_by_uuid() {
        let recent = chrono::Utc::now().timestamp();
        let devices = vec![("abc-123".to_string(), "Phone".to_string(), Some(recent))];
        let hint = last_seen_hint(&devices, "abc-123").unwrap();
        assert!(hint.contains("Phone"), "{hint}");
        assert!(hint.contains("was last seen"), "{hint}");
    }

    #[test]
    fn unknown_query_yields_none() {
        let devices = vec![("abc-123".to_string(), "Phone".to_string(), None)];
        assert!(last_seen_hint(&devices, "nobody").is_none());
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
            let folders = ctx
                .storage
                .list_sync_folders()
                .map(|v| v.len())
                .unwrap_or(0);
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

async fn reset(state: &mut ReplState, ctx: &ApplicationContext, yes: bool) {
    if !yes {
        println!(
            "Factory reset restores this device to a fresh-install state:\n\
             \x20 - deletes the local identity (a new device id is generated on next start)\n\
             \x20 - unpairs every device\n\
             \x20 - removes all folders, shares, history and metadata\n\
             \x20 - keeps your local files untouched"
        );
        print!("Continue? [y/N] ");
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        if !crate::commands::input::read_yes_no().await {
            println!("Aborted.");
            return;
        }
    }

    state.stop_all().await;
    match ctx.reset().await {
        Ok(()) => println!(
            "Device reset to a fresh install. Restart the REPL to generate a new device id."
        ),
        Err(e) => eprintln!("error: {e:#}"),
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
        println!("  {}", peer.name);
    }
}

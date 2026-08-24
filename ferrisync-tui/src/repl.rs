use anyhow::{bail, Context, Result};
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::discovery::{DiscoveredPeer, DiscoveryService};
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::DeviceInfo;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context as RlContext, Editor, Helper};
use shlex::split as shlex_split;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ferrisync_core::sync_engine::server::{self, ServeHandle};
use ferrisync_core::sync_engine::SyncEvent;

use crate::cli::status as cli_status;
use crate::cli::sync as cli_sync;
use crate::cli::watch::{get_or_create_folder, watch_loop};
use crate::cli::{parse_device, DEFAULT_PORT};

const COMMANDS: &[&str] = &[
    "help", "status", "discover", "pair", "sync", "unsync", "watch", "watches", "unwatch", "serve",
    "serves", "unserve", "pendings", "confirm", "deny", "exit", "quit",
];
/// A parsed REPL input line.
#[derive(Debug, PartialEq)]
pub enum ReplCommand {
    Help,
    Status,
    Discover {
        seconds: u64,
    },
    Pair {
        ip: String,
        port: u16,
    },
    Sync {
        folder: Option<String>,
        device: Option<String>,
    },
    Unsync {
        folder: Option<String>,
        device: Option<String>,
        yes: bool,
    },
    Watch {
        folder: String,
        device: String,
    },
    Watches,
    Unwatch {
        id: u32,
    },
    Serve {
        folder: String,
        port: u16,
    },
    Serves,
    Unserve {
        id: u32,
    },
    Pendings,
    Confirm {
        n: u32,
    },
    Deny {
        n: u32,
    },
    Yes,
    No,
    Exit,
}

/// Tokenize and parse one input line. Returns `Ok(None)` for blank input.
pub fn parse_line(line: &str) -> Result<Option<ReplCommand>> {
    let Some(words) = shlex_split(line.trim()) else {
        bail!("unbalanced quotes");
    };
    let Some((cmd, args)) = words.split_first() else {
        return Ok(None);
    };

    let command = match cmd.as_str() {
        "help" | "?" => ReplCommand::Help,
        "exit" | "quit" | "q" => ReplCommand::Exit,
        "y" | "yes" => ReplCommand::Yes,
        "n" | "no" => ReplCommand::No,
        "status" => ReplCommand::Status,
        "watches" => ReplCommand::Watches,
        "discover" => {
            let seconds = match args.first() {
                None => 3,
                Some(s) => s
                    .parse()
                    .with_context(|| format!("invalid duration '{s}' (expected seconds)"))?,
            };
            ReplCommand::Discover { seconds }
        }
        "pair" => {
            let ip = args
                .first()
                .context("usage: pair <ip> [--port <port>]")?
                .clone();
            let port = match flag_value(args, "--port")? {
                None => DEFAULT_PORT,
                Some(p) => p.parse().with_context(|| format!("invalid port '{p}'"))?,
            };
            ReplCommand::Pair { ip, port }
        }
        "sync" => {
            let mut folder: Option<String> = None;
            let mut device: Option<String> = None;
            let mut it = args.iter();
            while let Some(tok) = it.next() {
                if tok == "--device" {
                    if device.is_some() {
                        bail!("duplicate --device");
                    }
                    device = Some(it.next().context("missing value for --device")?.clone());
                } else if tok.starts_with("--") {
                    bail!("unknown flag '{tok}' for sync");
                } else if folder.is_none() {
                    folder = Some(tok.clone());
                } else {
                    bail!("unexpected argument '{tok}'");
                }
            }
            match (folder, device) {
                (None, None) => ReplCommand::Sync {
                    folder: None,
                    device: None,
                },
                (Some(folder), Some(device)) => ReplCommand::Sync {
                    folder: Some(folder),
                    device: Some(device),
                },
                _ => bail!("usage: sync [<folder> --device <ip[:port]>]"),
            }
        }
        "unsync" => {
            let mut folder: Option<String> = None;
            let mut device: Option<String> = None;
            let mut yes = false;
            let mut it = args.iter();
            while let Some(tok) = it.next() {
                if tok == "--yes" {
                    if yes {
                        bail!("duplicate --yes");
                    }
                    yes = true;
                } else if tok == "--device" {
                    if device.is_some() {
                        bail!("duplicate --device");
                    }
                    device = Some(it.next().context("missing value for --device")?.clone());
                } else if tok.starts_with("--") {
                    bail!("unknown flag '{tok}' for unsync");
                } else if folder.is_none() {
                    folder = Some(tok.clone());
                } else {
                    bail!("unexpected argument '{tok}'");
                }
            }
            if yes && (folder.is_some() || device.is_some()) {
                bail!("--yes clears everything; it cannot be combined with a folder or --device");
            }
            ReplCommand::Unsync {
                folder,
                device,
                yes,
            }
        }
        "watch" => {
            let folder = args
                .first()
                .context("usage: watch <folder> --device <ip[:port]>")?
                .clone();
            let device = required_flag(args, "--device")?;
            ReplCommand::Watch { folder, device }
        }
        "unwatch" => {
            let id = args
                .first()
                .context("usage: unwatch <id>")?
                .parse()
                .with_context(|| "watch id must be a number")?;
            ReplCommand::Unwatch { id }
        }
        "serve" => {
            let folder = args
                .first()
                .context("usage: serve <folder> [--port <port>]")?
                .clone();
            let port = match flag_value(args, "--port")? {
                None => DEFAULT_PORT,
                Some(p) => p.parse().with_context(|| format!("invalid port '{p}'"))?,
            };
            ReplCommand::Serve { folder, port }
        }
        "serves" => ReplCommand::Serves,
        "unserve" => {
            let id = args
                .first()
                .context("usage: unserve <id>")?
                .parse()
                .with_context(|| "server id must be a number")?;
            ReplCommand::Unserve { id }
        }
        "pendings" => ReplCommand::Pendings,
        "confirm" => {
            let n = args
                .first()
                .context("usage: confirm <n> (see 'pendings')")?
                .parse()
                .with_context(|| "pairing number must be a number")?;
            ReplCommand::Confirm { n }
        }
        "deny" => {
            let n = args
                .first()
                .context("usage: deny <n> (see 'pendings')")?
                .parse()
                .with_context(|| "pairing number must be a number")?;
            ReplCommand::Deny { n }
        }
        other => bail!("unknown command: {other} (try 'help')"),
    };
    Ok(Some(command))
}

fn flag_value(args: &[String], name: &str) -> Result<Option<String>> {
    match args.iter().position(|a| a == name) {
        None => Ok(None),
        Some(i) => match args.get(i + 1) {
            Some(v) => Ok(Some(v.clone())),
            None => bail!("missing value for {name}"),
        },
    }
}

fn required_flag(args: &[String], name: &str) -> Result<String> {
    flag_value(args, name)?.with_context(|| format!("missing required flag {name}"))
}

struct ReplHelper;

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &RlContext,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let prefix = line.get(..pos).unwrap_or("");
        let start = prefix.rfind(' ').map_or(0, |i| i + 1);
        let word = &prefix[start..];

        let source: &[&str] = if start == 0 {
            COMMANDS
        } else if word.starts_with("--") {
            &["--device", "--port"]
        } else {
            &[]
        };

        let candidates = source
            .iter()
            .filter(|c| c.starts_with(word))
            .map(|c| Pair {
                display: (*c).to_string(),
                replacement: (*c).to_string(),
            })
            .collect();

        Ok((start, candidates))
    }
}

impl Hinter for ReplHelper {
    type Hint = String;
}

impl Highlighter for ReplHelper {}

impl Validator for ReplHelper {}

impl Helper for ReplHelper {}

struct WatchHandle {
    folder: String,
    device: String,
    shutdown: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

pub async fn run(
    pairing: PairingManager,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
    data_dir: &Path,
) -> Result<()> {
    let history_path: PathBuf = data_dir.join("repl_history");

    let mut rl = Editor::<ReplHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(ReplHelper));
    let _ = rl.load_history(&history_path);

    let mut watches: BTreeMap<u32, WatchHandle> = BTreeMap::new();
    let mut servers: BTreeMap<u32, ServeHandle> = BTreeMap::new();
    let mut next_id: u32 = 1;

    println!(
        "FerriSync {} — interactive shell",
        env!("CARGO_PKG_VERSION")
    );
    println!("Type 'help' for commands, 'exit' or Ctrl-D to quit.");

    loop {
        let readline = rl.readline("ferrisync> ");
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let _ = rl.add_history_entry(trimmed);
                }
                match parse_line(trimmed) {
                    Ok(None) => {}
                    Ok(Some(ReplCommand::Exit)) => break,
                    Ok(Some(ReplCommand::Help)) => print_help(),
                    Ok(Some(ReplCommand::Status)) => {
                        handle(cli_status::run(storage.clone(), device_info.clone()).await);
                    }
                    Ok(Some(ReplCommand::Discover { seconds })) => {
                        discover(&device_info, seconds).await;
                    }
                    Ok(Some(ReplCommand::Pair { ip, port })) => {
                        handle(crate::cli::pair::run(ip, port, &pairing).await);
                    }
                    Ok(Some(ReplCommand::Sync { folder, device })) => match (folder, device) {
                        (Some(folder), Some(device)) => {
                            handle(
                                cli_sync::run(folder, device, storage.clone(), crypto.clone())
                                    .await,
                            );
                        }
                        _ => {
                            handle(cli_sync::run_all(storage.clone(), crypto.clone()).await);
                        }
                    },
                    Ok(Some(ReplCommand::Unsync {
                        folder,
                        device,
                        yes,
                    })) => match (folder, device, yes) {
                        (None, None, false) => {
                            let folders = storage.list_sync_folders().map(|v| v.len()).unwrap_or(0);
                            let devices = storage.list_devices().map(|v| v.len()).unwrap_or(0);
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
                        (None, None, true) => match storage.clear_all_sync_state() {
                            Ok((f, d)) => {
                                println!(
                                    "Removed {f} folder entr{} and {d} device{} (metadata cleared).",
                                    if f == 1 { "y" } else { "ies" },
                                    if d == 1 { "" } else { "s" }
                                );
                                if !watches.is_empty() || !servers.is_empty() {
                                    println!("note: background watches/serves are still running; stop them with 'unwatch'/'unserve'");
                                }
                            }
                            Err(e) => eprintln!("error: {e:#}"),
                        },
                        (folder, device, false) => {
                            let Some(folder) = folder else {
                                unreachable!("scoped unsync requires a folder");
                            };
                            match storage.remove_sync_folders(&folder, device.as_deref()) {
                                Ok(0) => println!("no sync entries for '{folder}'"),
                                Ok(n) => println!(
                                    "removed {n} sync entr{} for '{folder}'",
                                    if n == 1 { "y" } else { "ies" }
                                ),
                                Err(e) => eprintln!("error: {e:#}"),
                            }
                        }
                        (_, _, true) => unreachable!("--yes conflicts rejected at parse time"),
                    },
                    Ok(Some(ReplCommand::Watch { folder, device })) => {
                        start_watch(
                            &mut watches,
                            &mut next_id,
                            folder,
                            device,
                            storage.clone(),
                            crypto.clone(),
                        );
                    }
                    Ok(Some(ReplCommand::Watches)) => list_watches(&watches),
                    Ok(Some(ReplCommand::Unwatch { id })) => {
                        stop_watch(&mut watches, id).await;
                    }
                    Ok(Some(ReplCommand::Serve { folder, port })) => {
                        start_server(
                            &mut servers,
                            &mut next_id,
                            folder,
                            port,
                            storage.clone(),
                            crypto.clone(),
                            device_info.clone(),
                        )
                        .await;
                    }
                    Ok(Some(ReplCommand::Serves)) => list_servers(&servers),
                    Ok(Some(ReplCommand::Unserve { id })) => {
                        stop_server(&mut servers, id).await;
                    }
                    Ok(Some(ReplCommand::Pendings)) => list_pendings(&servers),
                    Ok(Some(ReplCommand::Confirm { n })) => {
                        resolve_pending(&mut servers, n, true);
                    }
                    Ok(Some(ReplCommand::Deny { n })) => {
                        resolve_pending(&mut servers, n, false);
                    }
                    Ok(Some(ReplCommand::Yes)) => answer_latest(&mut servers, true),
                    Ok(Some(ReplCommand::No)) => answer_latest(&mut servers, false),
                    Err(e) => eprintln!("error: {e:#}"),
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("(to quit, press Ctrl-D or type 'exit')");
            }
            Err(ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        }
    }

    stop_all_watches(&mut watches).await;
    stop_all_servers(&mut servers).await;
    let _ = rl.save_history(&history_path);
    Ok(())
}

fn handle(result: Result<()>) {
    if let Err(e) = result {
        eprintln!("error: {e:#}");
    }
}

async fn discover(device_info: &DeviceInfo, seconds: u64) {
    println!("Scanning the LAN for FerriSync devices ({seconds}s)...");
    let service = match DiscoveryService::new(device_info.clone(), DEFAULT_PORT) {
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

fn start_watch(
    watches: &mut BTreeMap<u32, WatchHandle>,
    next_id: &mut u32,
    folder: String,
    device: String,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
) {
    let addr = match parse_device(&device, DEFAULT_PORT) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e:#}");
            return;
        }
    };
    let folder_id = match get_or_create_folder(&storage, &folder, &device) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("error: {e:#}");
            return;
        }
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let task_folder = folder.clone();
    let task = tokio::spawn(async move {
        if let Err(e) = watch_loop(
            task_folder.clone(),
            addr,
            folder_id,
            storage,
            crypto,
            shutdown_rx,
        )
        .await
        {
            eprintln!("[watch:{task_folder}] error: {e:#}");
        }
    });

    let id = *next_id;
    *next_id += 1;
    watches.insert(
        id,
        WatchHandle {
            folder: folder.clone(),
            device: device.clone(),
            shutdown: shutdown_tx,
            task,
        },
    );
    println!("watch #{id} started: {folder} ↔ {device} (background)");
}

fn list_watches(watches: &BTreeMap<u32, WatchHandle>) {
    if watches.is_empty() {
        println!("(no active background watches)");
        return;
    }
    for (id, w) in watches {
        println!("  #{id}  {} ↔ {}", w.folder, w.device);
    }
}

async fn stop_watch(watches: &mut BTreeMap<u32, WatchHandle>, id: u32) {
    match watches.remove(&id) {
        None => eprintln!("no such watch: #{id}"),
        Some(handle) => {
            let _ = handle.shutdown.send(true);
            await_shutdown(id, handle.task).await;
            println!("watch #{id} stopped");
        }
    }
}

async fn stop_all_watches(watches: &mut BTreeMap<u32, WatchHandle>) {
    let taken = std::mem::take(watches);
    for (id, w) in taken {
        let _ = w.shutdown.send(true);
        await_shutdown(id, w.task).await;
        println!("watch #{id} stopped");
    }
}

async fn start_server(
    servers: &mut BTreeMap<u32, ServeHandle>,
    next_id: &mut u32,
    folder: String,
    port: u16,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    device_info: DeviceInfo,
) {
    let (handle, mut events) = match server::serve_folder(
        storage,
        crypto,
        device_info,
        folder.clone(),
        port,
        server::PairPolicy::Confirm,
    )
    .await
    {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("error: {e:#}");
            return;
        }
    };

    let id = *next_id;
    *next_id += 1;

    // Drain sync events so the user sees activity from served folders.
    let task_folder = folder.clone();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                SyncEvent::PairRequested { name, .. } => {
                    print!("{}", pairing_notice_text(&task_folder, &name));
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
                SyncEvent::DevicePaired { name, .. } => {
                    println!("[serve:{task_folder}] paired with {name}");
                }
                SyncEvent::FilePushed { path, device } => {
                    println!("[serve:{task_folder}] pushed {path} -> {device}");
                }
                SyncEvent::FilePulled { path, device } => {
                    println!("[serve:{task_folder}] pulled {path} <- {device}");
                }
                SyncEvent::Conflict { path, .. } => {
                    println!("[serve:{task_folder}] conflict on {path}");
                }
                _ => {}
            }
        }
    });

    println!(
        "serve #{id} started: {} on 0.0.0.0:{} (background)\n  \
         unknown devices must be approved before they can pair — watch for \
         PAIRING REQUEST lines",
        handle.folder, handle.port
    );
    servers.insert(id, handle);
}

fn list_servers(servers: &BTreeMap<u32, ServeHandle>) {
    if servers.is_empty() {
        println!("(no active background servers)");
        return;
    }
    for (id, s) in servers {
        println!("  #{id}  {} on 0.0.0.0:{}", s.folder, s.port);
    }
}

async fn stop_server(servers: &mut BTreeMap<u32, ServeHandle>, id: u32) {
    match servers.remove(&id) {
        None => eprintln!("no such server: #{id}"),
        Some(handle) => {
            handle.stop().await;
            println!("server #{id} stopped");
        }
    }
}

async fn stop_all_servers(servers: &mut BTreeMap<u32, ServeHandle>) {
    let taken = std::mem::take(servers);
    for (id, s) in taken {
        s.stop().await;
        println!("server #{id} stopped");
    }
}

/// Held pairing requests across all servers: `(server_id, name, device_id)`,
/// numbered 1..n in the order `list_pendings` displays them.
fn collect_pending(servers: &BTreeMap<u32, ServeHandle>) -> Vec<(u32, String, String)> {
    let mut out = Vec::new();
    for (sid, s) in servers {
        match s.pending_pairings() {
            Ok(pending) => {
                for (name, id) in pending {
                    out.push((*sid, name, id));
                }
            }
            Err(e) => eprintln!("error: {e:#}"),
        }
    }
    out
}

fn list_pendings(servers: &BTreeMap<u32, ServeHandle>) {
    print!("{}", format_pendings(&collect_pending(servers)));
}

fn format_pendings(all: &[(u32, String, String)]) -> String {
    if all.is_empty() {
        return "(no pairing requests waiting)\n".to_string();
    }
    let mut out = String::new();
    for (i, (_, name, id)) in all.iter().enumerate() {
        out.push_str(&format!("  {}  {name} ({id})\n", i + 1));
    }
    out
}

/// Notice printed when an unknown device is held for pairing approval.
/// Starts with a newline so it stays readable even if a REPL prompt or
/// serve log line was mid-output.
fn pairing_notice_text(folder: &str, name: &str) -> String {
    format!(
        "\n[serve:{folder}] PAIRING REQUEST — confirm connection with '{name}'?\n  \
         `y` allows, `n` denies (`pendings` lists held requests)\n"
    )
}

/// Answer the single held pairing request with y/n; refuses to guess when
/// several requests are waiting.
fn answer_latest(servers: &mut BTreeMap<u32, ServeHandle>, approve: bool) {
    match collect_pending(servers).len() {
        0 => println!("(no pairing requests waiting)"),
        1 => resolve_pending(servers, 1, approve),
        _ => {
            println!("multiple requests waiting — use 'confirm <n>' or 'deny <n>' (see 'pendings')")
        }
    }
}

fn resolve_pending(servers: &mut BTreeMap<u32, ServeHandle>, n: u32, approve: bool) {
    let all = collect_pending(servers);
    if n == 0 || n as usize > all.len() {
        eprintln!("no such pairing request: {n} (see 'pendings')");
        return;
    }
    let (sid, name, id) = all[(n - 1) as usize].clone();
    let Some(server) = servers.get_mut(&sid) else {
        return;
    };
    let result = if approve {
        server
            .approve_pairing(&id, &name)
            .map(|_| format!("approved '{name}' — they can now pair"))
    } else {
        server.deny_pairing(&id).map(|_| format!("denied '{name}'"))
    };
    match result {
        Ok(msg) => println!("{msg}"),
        Err(e) => eprintln!("error: {e:#}"),
    }
}

async fn await_shutdown(id: u32, task: tokio::task::JoinHandle<()>) {
    match tokio::time::timeout(Duration::from_secs(5), task).await {
        Ok(_) => {}
        Err(_) => {
            log::warn!("[watch:{id}] did not stop within 5s, aborting");
        }
    }
}

fn print_help() {
    println!(
        "Commands:
  help                          Show this help
  status                        Show paired devices and sync folders
  discover [seconds]            Scan the LAN for FerriSync devices (default 3s)
  pair <ip> [--port <port>]     Pair with a device (default port {DEFAULT_PORT})
  sync                          Sync ALL configured folders
  sync <folder> --device <ip[:port]>
                                One-shot folder sync
  unsync                        Show what a full reset would remove
  unsync --yes                  Clear ALL folders, devices, and sync metadata
  unsync <folder> [--device <id>]
                                Remove sync entries for a folder
  watch <folder> --device <ip[:port]>
                                Sync on every change (runs in background)
   watches                       List background watches
   unwatch <id>                  Stop a background watch
   serve <folder> [--port <port>]
                                 Host the folder for pairing + sync (background)
   serves                        List background servers
   unserve <id>                  Stop a background server
   pendings                      List devices waiting for pairing approval
   confirm <n>                   Approve a held pairing request
   deny <n>                      Deny a held pairing request
   y / n                         Answer the single held pairing request
   exit                          Leave the shell (also: quit, Ctrl-D)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Option<ReplCommand> {
        parse_line(line).unwrap()
    }

    #[test]
    fn blank_input_is_none() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("   "), None);
    }

    #[test]
    fn simple_commands() {
        assert_eq!(parse("help"), Some(ReplCommand::Help));
        assert_eq!(parse("?"), Some(ReplCommand::Help));
        assert_eq!(parse("status"), Some(ReplCommand::Status));
        assert_eq!(parse("exit"), Some(ReplCommand::Exit));
        assert_eq!(parse("quit"), Some(ReplCommand::Exit));
        assert_eq!(parse("q"), Some(ReplCommand::Exit));
        assert_eq!(parse("watches"), Some(ReplCommand::Watches));
    }

    #[test]
    fn unknown_command_errors() {
        assert!(parse_line("frobnicate").is_err());
    }

    #[test]
    fn comments_are_stripped() {
        assert_eq!(parse("#"), None);
        assert_eq!(parse("status # check state"), Some(ReplCommand::Status));
    }

    #[test]
    fn unbalanced_quotes_error() {
        assert!(parse_line("sync \"/home/x/My Docs").is_err());
    }

    #[test]
    fn quoted_paths_are_one_token() {
        assert_eq!(
            parse(r#"sync "/home/x/My Docs" --device 192.168.1.5"#),
            Some(ReplCommand::Sync {
                folder: Some("/home/x/My Docs".into()),
                device: Some("192.168.1.5".into()),
            })
        );
    }

    #[test]
    fn pair_defaults_and_port_flag() {
        assert_eq!(
            parse("pair 192.168.1.42"),
            Some(ReplCommand::Pair {
                ip: "192.168.1.42".into(),
                port: DEFAULT_PORT,
            })
        );
        assert_eq!(
            parse("pair 192.168.1.42 --port 9000"),
            Some(ReplCommand::Pair {
                ip: "192.168.1.42".into(),
                port: 9000,
            })
        );
        assert!(parse_line("pair").is_err());
        assert!(parse_line("pair 1.2.3.4 --port").is_err());
        assert!(parse_line("pair 1.2.3.4 --port abc").is_err());
    }

    #[test]
    fn sync_bare_and_explicit_forms() {
        assert_eq!(
            parse("sync"),
            Some(ReplCommand::Sync {
                folder: None,
                device: None,
            })
        );
        assert_eq!(
            parse("sync ~/Documents --device 10.0.0.2:7000"),
            Some(ReplCommand::Sync {
                folder: Some("~/Documents".into()),
                device: Some("10.0.0.2:7000".into()),
            })
        );
    }

    #[test]
    fn sync_rejects_partial_args() {
        assert!(parse_line("sync ~/Documents").is_err());
        assert!(parse_line("sync --device 10.0.0.2").is_err());
        assert!(parse_line("sync ~/Documents --device").is_err());
    }

    #[test]
    fn unsync_folder_with_optional_device() {
        assert_eq!(
            parse("unsync test"),
            Some(ReplCommand::Unsync {
                folder: Some("test".into()),
                device: None,
                yes: false,
            })
        );
        assert_eq!(
            parse("unsync test --device a5c13877"),
            Some(ReplCommand::Unsync {
                folder: Some("test".into()),
                device: Some("a5c13877".into()),
                yes: false,
            })
        );
        assert!(parse_line("unsync test extra").is_err());
        assert!(parse_line("unsync --device").is_err());
        assert!(parse_line("unsync --bogus").is_err());
    }

    #[test]
    fn unsync_full_reset_requires_confirmation() {
        assert_eq!(
            parse("unsync"),
            Some(ReplCommand::Unsync {
                folder: None,
                device: None,
                yes: false,
            })
        );
        assert_eq!(
            parse("unsync --yes"),
            Some(ReplCommand::Unsync {
                folder: None,
                device: None,
                yes: true,
            })
        );
        // --yes cannot be scoped.
        assert!(parse_line("unsync test --yes").is_err());
        assert!(parse_line("unsync --yes --yes").is_err());
    }

    #[test]
    fn watch_requires_device_flag() {
        assert!(parse_line("watch ~/Photos").is_err());
        assert_eq!(
            parse("watch ~/Photos --device 10.0.0.2"),
            Some(ReplCommand::Watch {
                folder: "~/Photos".into(),
                device: "10.0.0.2".into(),
            })
        );
    }

    #[test]
    fn discover_default_and_custom_seconds() {
        assert_eq!(
            parse("discover"),
            Some(ReplCommand::Discover { seconds: 3 })
        );
        assert_eq!(
            parse("discover 10"),
            Some(ReplCommand::Discover { seconds: 10 })
        );
        assert!(parse_line("discover soon").is_err());
    }

    #[test]
    fn unwatch_needs_a_number() {
        assert!(parse_line("unwatch abc").is_err());
        assert!(parse_line("unwatch").is_err());
        assert_eq!(parse("unwatch 3"), Some(ReplCommand::Unwatch { id: 3 }));
    }

    #[test]
    fn serve_defaults_and_port_flag() {
        assert_eq!(
            parse("serve ~/Sync"),
            Some(ReplCommand::Serve {
                folder: "~/Sync".into(),
                port: DEFAULT_PORT,
            })
        );
        assert_eq!(
            parse(r#"serve "~/My Docs" --port 7000"#),
            Some(ReplCommand::Serve {
                folder: "~/My Docs".into(),
                port: 7000,
            })
        );
        assert!(parse_line("serve").is_err());
        assert!(parse_line("serve ~/x --port").is_err());
        assert!(parse_line("serve ~/x --port abc").is_err());
    }

    #[test]
    fn serves_and_unserve() {
        assert_eq!(parse("serves"), Some(ReplCommand::Serves));
        assert!(parse_line("unserve").is_err());
        assert!(parse_line("unserve abc").is_err());
        assert_eq!(parse("unserve 2"), Some(ReplCommand::Unserve { id: 2 }));
    }

    #[test]
    fn pendings_confirm_deny() {
        assert_eq!(parse("pendings"), Some(ReplCommand::Pendings));
        assert_eq!(parse("confirm 1"), Some(ReplCommand::Confirm { n: 1 }));
        assert_eq!(parse("deny 3"), Some(ReplCommand::Deny { n: 3 }));
        assert!(parse_line("confirm").is_err());
        assert!(parse_line("confirm abc").is_err());
        assert!(parse_line("deny").is_err());
    }

    #[test]
    fn yes_no_shortcuts() {
        assert_eq!(parse("y"), Some(ReplCommand::Yes));
        assert_eq!(parse("yes"), Some(ReplCommand::Yes));
        assert_eq!(parse("n"), Some(ReplCommand::No));
        assert_eq!(parse("no"), Some(ReplCommand::No));
    }

    #[test]
    fn pairing_notice_contains_all_guidance() {
        let text = pairing_notice_text("myfolder", "Pixel 7");
        assert!(
            text.starts_with('\n'),
            "notice should start on a fresh line"
        );
        assert!(text.contains("[serve:myfolder]"));
        assert!(text.contains("confirm connection with 'Pixel 7'?"));
        assert!(text.contains("`y` allows"));
        assert!(text.contains("`n` denies"));
        assert!(text.contains("`pendings`"));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn format_pendings_empty_and_numbered() {
        assert_eq!(format_pendings(&[]), "(no pairing requests waiting)\n");

        let entries = vec![
            (7u32, "phone-a".to_string(), "id-a".to_string()),
            (9u32, "phone-b".to_string(), "id-b".to_string()),
        ];
        let text = format_pendings(&entries);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("  1  "));
        assert!(lines[0].contains("phone-a") && lines[0].contains("(id-a)"));
        assert!(lines[1].starts_with("  2  "));
        assert!(lines[1].contains("phone-b") && lines[1].contains("(id-b)"));
    }
}

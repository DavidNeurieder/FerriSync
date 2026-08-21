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
    "help", "status", "discover", "pair", "sync", "watch", "watches", "unwatch", "serve", "serves",
    "unserve", "exit", "quit",
];

/// A parsed REPL input line.
#[derive(Debug, PartialEq)]
pub enum ReplCommand {
    Help,
    Status,
    Discover { seconds: u64 },
    Pair { ip: String, port: u16 },
    Sync { folder: String, device: String },
    Watch { folder: String, device: String },
    Watches,
    Unwatch { id: u32 },
    Serve { folder: String, port: u16 },
    Serves,
    Unserve { id: u32 },
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
            let folder = args
                .first()
                .context("usage: sync <folder> --device <ip[:port]>")?
                .clone();
            let device = required_flag(args, "--device")?;
            ReplCommand::Sync { folder, device }
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
                    Ok(Some(ReplCommand::Sync { folder, device })) => {
                        handle(
                            cli_sync::run(folder, device, storage.clone(), crypto.clone()).await,
                        );
                    }
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
    let (handle, mut events) =
        match server::serve_folder(storage, crypto, device_info, folder.clone(), port).await {
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
        "serve #{id} started: {} on 0.0.0.0:{} (background)",
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
    for (_, s) in taken {
        s.stop().await;
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
  sync <folder> --device <ip[:port]>
                                One-shot folder sync
  watch <folder> --device <ip[:port]>
                                Sync on every change (runs in background)
   watches                       List background watches
   unwatch <id>                  Stop a background watch
   serve <folder> [--port <port>]
                                 Host the folder for pairing + sync (background)
   serves                        List background servers
   unserve <id>                  Stop a background server
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
                folder: "/home/x/My Docs".into(),
                device: "192.168.1.5".into(),
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
    fn sync_requires_device_flag() {
        assert!(parse_line("sync ~/Documents").is_err());
        assert_eq!(
            parse("sync ~/Documents --device 10.0.0.2:7000"),
            Some(ReplCommand::Sync {
                folder: "~/Documents".into(),
                device: "10.0.0.2:7000".into(),
            })
        );
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
}

use clap::{Parser, Subcommand};
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::{
    load_device_name, persist_device_name, sanitize_device_name, CryptoProvider, DeviceInfo,
    PairPolicy, PairingManager, SyncEngine, SyncEvent,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "ferrisync", version, about = "Decentralized folder sync")]
struct Cli {
    /// Data directory (default: ~/.local/share/ferrisync)
    #[arg(long, default_value = "")]
    data_dir: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Pair with a device by IP address
    Pair {
        /// IP address of the target device
        ip: String,
        /// Port (default: 9847)
        #[arg(long, default_value = "9847")]
        port: u16,
    },
    /// One-shot folder sync (no args: sync all configured folders)
    Sync {
        /// Local folder path
        #[arg(requires = "device")]
        folder: Option<String>,
        /// Target device ID
        #[arg(long, requires = "folder")]
        device: Option<String>,
        /// Keep retrying an unreachable peer for this many seconds
        #[arg(long, default_value_t = 0)]
        wait: u64,
    },
    /// Show pairing and sync status
    Status,
    /// Continuous foreground sync with live log
    Watch {
        /// Local folder path
        folder: String,
        /// Remote device address (IP:port)
        #[arg(long)]
        device: String,
    },
    /// Listen for incoming sync connections
    Serve {
        /// Listen port (default: 9847)
        #[arg(long, default_value = "9847")]
        port: u16,
        /// Accept pairing requests from unknown devices without confirmation
        #[arg(long)]
        auto_accept: bool,
        /// Local folder path to serve
        folder: String,
    },
    /// Change this device's network name (visible to peers)
    Rename {
        /// The new display name (max 64 characters)
        name: String,
    },
    /// Remove a paired device and all its associated data
    Remove {
        /// Device ID (run `ferrisync status` to see paired IDs)
        device_id: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

fn data_dir(cli: &Cli) -> PathBuf {
    if !cli.data_dir.is_empty() {
        PathBuf::from(&cli.data_dir)
    } else {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ferrisync")
    }
}

/// Read one line from stdin and interpret it as an approval decision.
/// Anything other than y/yes counts as "no" (including EOF).
async fn read_yes_no() -> bool {
    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    let _ = tokio::io::BufReader::new(tokio::io::stdin())
        .read_line(&mut line)
        .await;
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Stable per-data-dir identity: the TLS keypair is persisted so paired
/// devices recognize us across restarts; the device id is derived from the
/// certificate fingerprint.
async fn load_or_create_crypto(data: &PathBuf) -> anyhow::Result<Arc<CryptoProvider>> {
    std::fs::create_dir_all(data)?;
    let cert_path = data.join("cert.der");
    let key_path = data.join("key.der");

    if cert_path.exists() && key_path.exists() {
        let cert_der = std::fs::read(&cert_path)?;
        let key_der = std::fs::read(&key_path)?;
        let fingerprint = blake3::hash(&cert_der).as_bytes().to_vec();
        let crypto = CryptoProvider::load(cert_der, key_der, fingerprint)?;
        Ok(Arc::new(crypto))
    } else {
        let crypto = CryptoProvider::generate()?;
        let cert = crypto.certificate().await;
        std::fs::write(&cert_path, cert.as_ref())?;
        std::fs::write(&key_path, crypto.private_key().await.secret_der())?;
        Ok(Arc::new(crypto))
    }
}

/// Deterministic UUID (v5-style layout) from the certificate fingerprint,
/// so a persisted keypair always yields the same device id.
fn device_id_from_fingerprint(fingerprint: &[u8]) -> String {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&fingerprint[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

fn load_or_create_storage(data: &PathBuf) -> anyhow::Result<Arc<ferrisync_core::Storage>> {
    std::fs::create_dir_all(data)?;
    let db_path = data.join("metadata.db");
    let storage = ferrisync_core::Storage::open(&db_path)?;
    Ok(Arc::new(storage))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let data = data_dir(&cli);

    let crypto = load_or_create_crypto(&data).await?;
    let storage = load_or_create_storage(&data)?;

    let cert_fingerprint = crypto.fingerprint().await;
    let dev_id = device_id_from_fingerprint(&cert_fingerprint);
    let device_info = DeviceInfo {
        id: dev_id,
        name: load_device_name(&data).unwrap_or_else(|| {
            whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string())
        }),
        cert_fingerprint,
    };

    let state_store = Arc::new(InMemoryStateStore::new());
    let engine = SyncEngine::new(storage.clone(), crypto.clone(), device_info.clone(), state_store);
    let pairing = PairingManager::new(crypto.clone(), storage.clone(), device_info.clone());

    match cli.command {
        Some(cmd) => {
            match cmd {
                Commands::Pair { ip, port } => {
                    let addr: SocketAddr = format!("{ip}:{port}").parse()?;
                    println!("Pairing with {addr}...");
                    match pairing.pair_with(addr).await {
                        Ok(peer) => {
                            println!("Paired with {} ({})", peer.name, peer.id);
                        }
                        Err(e) => {
                            eprintln!("Pairing failed: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Commands::Sync {
                    folder,
                    device,
                    wait,
                } => match (folder, device) {
                    (Some(folder), Some(device)) => {
                        storage.upsert_device(&device, &device, None, None)?;
                        let folder_id =
                            storage.add_sync_folder(&folder, &device, "bidirectional")?;
                        let addr: SocketAddr = if device.contains(':') {
                            device
                                .parse()
                                .map_err(|_| anyhow::anyhow!("invalid device address {device}"))?
                        } else {
                            format!("{device}:9847")
                                .parse()
                                .map_err(|_| anyhow::anyhow!("invalid device address {device}"))?
                        };
                        println!("Syncing {folder} with device {addr}...");
                        let deadline =
                            std::time::Instant::now() + std::time::Duration::from_secs(wait);
                        let mut waiting = false;
                        let result = loop {
                            match engine
                                .run_sync(&folder, addr, folder_id, &device)
                                .await
                            {
                                ok @ Ok(_) => break ok,
                                Err(e)
                                    if wait > 0
                                        && e.to_string().contains("could not reach")
                                        && std::time::Instant::now() < deadline =>
                                {
                                    if !waiting {
                                        println!(
                                            "Waiting up to {wait}s for the peer to become reachable…"
                                        );
                                        waiting = true;
                                    }
                                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                }
                                err => break err,
                            }
                        };
                        match result {
                            Ok(result) => {
                                println!(
                                    "Sync complete. Pushed: {} files, Pulled: {} files",
                                    result.pushed.len(),
                                    result.pulled.len(),
                                );
                            }
                            Err(e) => {
                                eprintln!("Sync failed: {e}");
                                std::process::exit(1);
                            }
                        }
                    }
                    (None, None) => {
                        let outcomes = engine.sync_all_folders().await?;
                        if outcomes.is_empty() {
                            println!("No sync folders configured.");
                        }
                        let mut synced = 0usize;
                        let mut failed = 0usize;
                        for outcome in &outcomes {
                            match (&outcome.addr, &outcome.result) {
                                (None, _) => {
                                    println!(
                                        "Skipped {} — no known address for device {}; pair or discover first.",
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
                                    println!(
                                        "Synced {} with {}. Pushed: {} files, Pulled: {} files{}",
                                        outcome.path,
                                        addr,
                                        result.pushed.len(),
                                        result.pulled.len(),
                                        loopback_hint,
                                    );
                                }
                                (Some(_), Some(Err(e))) => {
                                    failed += 1;
                                    println!("Failed to sync {}: {e}", outcome.path);
                                }
                                (Some(_), None) => {
                                    unreachable!("session ran but produced no result")
                                }
                            }
                        }
                        if synced == 0 && failed > 0 && !outcomes.is_empty() {
                            std::process::exit(1);
                        }
                    }
                    _ => anyhow::bail!("usage: ferrisync sync [<folder> --device <id>]"),
                },
                Commands::Status => {
                    let devices = storage.list_devices()?;
                    println!("Paired devices:");
                    if devices.is_empty() {
                        println!("  (none)");
                    }
                    for (id, name, last_seen) in &devices {
                        let last = last_seen.map_or("never".to_string(), |ts| {
                            chrono::DateTime::from_timestamp(ts, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                .unwrap_or_else(|| ts.to_string())
                        });
                        println!("  {name} ({id}) — last seen: {last}");
                    }

                    let folders = storage.list_sync_folders()?;
                    println!("Sync folders:");
                    if folders.is_empty() {
                        println!("  (none)");
                    }
                    for (id, path, dev_id, dir, last_sync) in &folders {
                        let last = last_sync.map_or("never".to_string(), |ts| {
                            chrono::DateTime::from_timestamp(ts, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                .unwrap_or_else(|| ts.to_string())
                        });
                        println!("  [{id}] {path} ↔ {dev_id} ({dir}) — last sync: {last}");
                    }

                    println!();
                    println!("Device ID: {}", device_info.id);
                    println!("Device name: {}", device_info.name);
                }
                Commands::Watch { folder, device } => {
                    println!("Watching {folder} for changes, syncing to {device}...");
                    let folder_id = storage.add_sync_folder(&folder, &device, "bidirectional")?;
                    let remote_addr: SocketAddr = device.parse()?;
                    let (trigger_tx, mut trigger_rx) = tokio::sync::mpsc::channel(256);
                    let watch_folder = folder.clone();

                    let scheduler = ferrisync_core::ChangeScheduler::new(
                        std::time::Duration::from_millis(500),
                        0,
                    );

                    tokio::spawn(async move {
                        use ferrisync_core::FileWatcher;
                        let mut watcher = match FileWatcher::watch(PathBuf::from(&watch_folder)) {
                            Ok(w) => w,
                            Err(e) => {
                                log::error!("Failed to watch {watch_folder}: {e}");
                                return;
                            }
                        };
                        // Bridge raw events into the scheduler
                        scheduler.run(watcher.events(), &trigger_tx).await;
                    });

                    println!("Watching... (press Ctrl+C to stop)");
                    loop {
                        tokio::select! {
                            _ = trigger_rx.recv() => {
                                println!("Change detected, syncing...");
                                match engine.run_sync(
                                    &folder,
                                    remote_addr,
                                    folder_id,
                                    &device,
                                ).await {
                                    Ok(result) => {
                                        println!("Sync complete. Pushed: {}, Pulled: {}, Conflicts: {}",
                                            result.pushed.len(),
                                            result.pulled.len(),
                                            result.conflicts.len(),
                                        );
                                    }
                                    Err(e) => {
                                        eprintln!("Sync failed: {e}");
                                    }
                                }
                            }
                            _ = tokio::signal::ctrl_c() => {
                                println!("Shutting down.");
                                break;
                            }
                        }
                    }
                }
                Commands::Serve {
                    port,
                    folder,
                    auto_accept,
                } => {
                    use std::io::IsTerminal;
                    let interactive = std::io::stdin().is_terminal() && !auto_accept;
                    let policy = if interactive {
                        PairPolicy::Confirm
                    } else {
                        PairPolicy::AutoAccept
                    };
                    let (server, mut events) = engine
                        .serve_folder(folder.clone(), port, policy)
                        .await?;
                    println!("Serving folder \"{folder}\" on 0.0.0.0:{}", server.port);
                    println!("Advertising on mDNS as _ferrisync._tcp");
                    if interactive {
                        println!("Unknown devices need your confirmation before pairing.");
                    } else {
                        println!("Auto-accepting pairing requests (use --auto-accept or run without a TTY).");
                        eprintln!(
                            "WARNING: --auto-accept trusts ANY device on the network and gives it \
                             read/write access to this folder. Only use it on trusted networks."
                        );
                    }
                    println!("Serve mode active. Press Ctrl+C to stop.");

                    tokio::select! {
                        _ = async {
                            while let Some(event) = events.recv().await {
                                match event {
                                    SyncEvent::PairRequested { name, id } => {
                                        println!();
                                        print!(
                                            "Confirm connection with '{name}' ({id})? [y/N] "
                                        );
                                        use std::io::Write as _;
                                        let _ = std::io::stdout().flush();
                                        let answer = read_yes_no().await;
                                        if answer {
                                            if let Err(e) = server.approve_pairing(&id, &name) {
                                                println!("approve failed: {e:#}");
                                            }
                                            println!("\nApproved '{name}'. They can now pair.");
                                        } else {
                                            if let Err(e) = server.deny_pairing(&id) {
                                                println!("deny failed: {e:#}");
                                            }
                                            println!("\nDenied '{name}'.");
                                        }
                                    }
                                    SyncEvent::DevicePaired { name, .. } => {
                                        println!("[serve] paired with {name}");
                                    }
                                    SyncEvent::FilePushed { path, device } => {
                                        println!("[serve] pushed {path} -> {device}");
                                    }
                                    SyncEvent::FilePulled { path, device } => {
                                        println!("[serve] pulled {path} <- {device}");
                                    }
                                    SyncEvent::Conflict { path, .. } => {
                                        println!("[serve] conflict on {path}");
                                    }
                                    _ => {}
                                }
                            }
                        } => {}
                        _ = tokio::signal::ctrl_c() => {}
                    }

                    server.stop().await;
                    println!("Shutting down.");
                }
                Commands::Remove { device_id, yes } => {
                    // Look up device name before deletion for the prompt.
                    let device_name = storage
                        .list_devices()?
                        .into_iter()
                        .find(|(id, _, _)| id == &device_id)
                        .map(|(_, name, _)| name);

                    match &device_name {
                        Some(name) => {
                            if !yes {
                                println!(
                                    "Remove device '{name}' ({device_id})? \
                                     This deletes all associated folders and history."
                                );
                                print!("Continue? [y/N] ");
                                use std::io::Write as _;
                                let _ = std::io::stdout().flush();
                                if !read_yes_no().await {
                                    println!("Aborted.");
                                    return Ok(());
                                }
                            }
                        }
                        None => {
                            eprintln!("Device {device_id} not found.");
                            std::process::exit(1);
                        }
                    }

                    let c = storage.remove_device(&device_id)?;
                    println!("Removed device '{}'.", device_name.unwrap_or_default());
                    if c.folders_removed > 0 {
                        println!("  {} folder(s) deleted", c.folders_removed);
                    }
                    if c.sessions_removed > 0 {
                        println!("  {} session(s) deleted", c.sessions_removed);
                    }
                    if c.history_removed > 0 {
                        println!("  {} history entry/entries deleted", c.history_removed);
                    }
                    if c.metadata_removed > 0 {
                        println!("  {} file metadata entry/entries deleted", c.metadata_removed);
                    }
                }
                Commands::Rename { name } => {
                    match sanitize_device_name(&name) {
                        Ok(clean) => {
                            persist_device_name(&data, &clean);
                            println!("Renamed to '{clean}'.");
                            println!(
                                "Already-running 'serve' processes keep the old name until restarted."
                            );
                        }
                        Err(e) => {
                            eprintln!("error: {e:#}");
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        None => {
            println!(
                "FerriSync {} — decentralized folder sync",
                env!("CARGO_PKG_VERSION")
            );
            println!("Run `ferrisync --help` for available commands.");
            println!();
            println!("Device ID: {}", device_info.id);
            println!("Device name: {}", device_info.name);
            println!("Data directory: {}", data.display());
        }
    }

    Ok(())
}

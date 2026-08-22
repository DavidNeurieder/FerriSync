use clap::{Parser, Subcommand};
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::session;
use ferrisync_core::sync_engine::SyncEvent;
use ferrisync_core::DeviceInfo;
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
    /// One-shot folder sync
    Sync {
        /// Local folder path
        folder: String,
        /// Target device ID
        #[arg(long)]
        device: String,
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

fn load_or_create_storage(data: &PathBuf) -> anyhow::Result<Arc<Storage>> {
    std::fs::create_dir_all(data)?;
    let db_path = data.join("metadata.db");
    let storage = Storage::open(&db_path)?;
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
        name: whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string()),
        cert_fingerprint,
    };

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
                Commands::Sync { folder, device } => {
                    storage.upsert_device(&device, &device, None)?;
                    let folder_id = storage.add_sync_folder(&folder, &device, "bidirectional")?;
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
                    let (event_tx, _event_rx) = tokio::sync::mpsc::channel(256);
                    match session::run_sync_session(
                        crypto.clone(),
                        storage.clone(),
                        &folder,
                        addr,
                        folder_id,
                        &device,
                        event_tx,
                    )
                    .await
                    {
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
                    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(256);
                    let watch_folder = folder.clone();

                    tokio::spawn(async move {
                        use ferrisync_core::watcher::FileWatcher;
                        let mut watcher = match FileWatcher::watch(PathBuf::from(&watch_folder)) {
                            Ok(w) => w,
                            Err(e) => {
                                log::error!("Failed to watch {watch_folder}: {e}");
                                return;
                            }
                        };
                        while let Some(_event) = watcher.events().recv().await {
                            let _ = event_tx.send(()).await;
                        }
                    });

                    println!("Watching... (press Ctrl+C to stop)");
                    loop {
                        tokio::select! {
                            _ = event_rx.recv() => {
                                // Debounce: wait briefly for more events before syncing
                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                while event_rx.try_recv().is_ok() {}
                                println!("Change detected, syncing...");
                                match session::run_sync_session(
                                    crypto.clone(),
                                    storage.clone(),
                                    &folder,
                                    remote_addr,
                                    folder_id,
                                    &device,
                                    tokio::sync::mpsc::channel(256).0,
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
                        ferrisync_core::sync_engine::server::PairPolicy::Confirm
                    } else {
                        ferrisync_core::sync_engine::server::PairPolicy::AutoAccept
                    };
                    let (server, mut events) = ferrisync_core::sync_engine::server::serve_folder(
                        storage.clone(),
                        crypto.clone(),
                        device_info.clone(),
                        folder.clone(),
                        port,
                        policy,
                    )
                    .await?;
                    println!("Serving folder \"{folder}\" on 0.0.0.0:{}", server.port);
                    println!("Advertising on mDNS as _ferrisync._tcp");
                    if interactive {
                        println!("Unknown devices need your confirmation before pairing.");
                    } else {
                        println!("Auto-accepting pairing requests (use --auto-accept or run without a TTY).");
                    }
                    println!("Serve mode active. Press Ctrl+C to stop.");

                    tokio::select! {
                        _ = async {
                            while let Some(event) = events.recv().await {
                                match event {
                                    SyncEvent::PairRequested { name, id } => {
                                        println!();
                                        print!(
                                            "Pairing request from '{name}' ({id}). Allow? [y/N] "
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

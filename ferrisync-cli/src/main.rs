use clap::{Parser, Subcommand};
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::session;
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
    },
    /// Listen for incoming sync connections
    Serve {
        /// Listen port (default: 9847)
        #[arg(long, default_value = "9847")]
        port: u16,
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

fn load_or_create_crypto(data: &PathBuf) -> anyhow::Result<Arc<CryptoProvider>> {
    std::fs::create_dir_all(data)?;
    let cert_path = data.join("cert.der");
    let key_path = data.join("key.der");

    if cert_path.exists() && key_path.exists() {
        // Load existing
        let cert_der = std::fs::read(&cert_path)?;
        let key_der = std::fs::read(&key_path)?;
        let fingerprint = blake3::hash(&cert_der).as_bytes().to_vec();
        let crypto = CryptoProvider::load(cert_der, key_der, fingerprint)?;
        Ok(Arc::new(crypto))
    } else {
        // Generate new
        let crypto = CryptoProvider::generate()?;
        // Save cert and key
        // For simplicity, we regenerate on each run (production would save)
        Ok(Arc::new(crypto))
    }
}

fn load_or_create_storage(data: &PathBuf) -> anyhow::Result<Arc<Storage>> {
    std::fs::create_dir_all(data)?;
    let db_path = data.join("metadata.db");
    let storage = Storage::open(&db_path)?;
    Ok(Arc::new(storage))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let data = data_dir(&cli);

    let crypto = load_or_create_crypto(&data)?;
    let storage = load_or_create_storage(&data)?;

    let dev_id = uuid::Uuid::new_v4().to_string();
    let device_info = DeviceInfo {
        id: dev_id,
        name: whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string()),
        cert_fingerprint: crypto.fingerprint().await,
    };

    let pairing = PairingManager::new(crypto.clone(), storage.clone(), device_info.clone());

    match cli.command {
        Some(cmd) => match cmd {
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
                let folder_id = storage.add_sync_folder(&folder, &device, "bidirectional")?;
                let addr: SocketAddr = format!("{device}:9847")
                    .parse()
                    .map_err(|_| anyhow::anyhow!("device must be an IP:port, got {device}"))?;
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
            Commands::Watch { folder } => {
                println!("Watching {folder} for changes...");
                let (tx, mut rx) = tokio::sync::mpsc::channel(256);
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
                    while let Some(event) = watcher.events().recv().await {
                        let _ = tx.send(event).await;
                    }
                });

                println!("Watching... (press Ctrl+C to stop)");
                while let Some(event) = rx.recv().await {
                    println!("Change detected: {event:?}");
                    // Trigger sync for each paired device
                    let devices = storage.list_devices()?;
                    for (dev_id, name, _) in &devices {
                        // For now, just log
                        println!("  → would sync with {name} ({dev_id})");
                    }
                }
            }
            Commands::Serve { port, folder } => {
                storage.upsert_device("serve", "serve-mode", None)?;
                let folder_id = storage.add_sync_folder(&folder, "serve", "bidirectional")?;
                let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
                println!("Serving folder \"{folder}\" on {addr}");
                let (event_tx, _event_rx) = tokio::sync::mpsc::channel(256);
                session::listen_for_sync(
                    crypto.clone(),
                    storage.clone(),
                    addr,
                    folder,
                    folder_id,
                    event_tx,
                )
                .await?;
                println!("Serve mode active. Press Ctrl+C to stop.");
                tokio::signal::ctrl_c().await?;
                println!("Shutting down.");
            }
        },
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

mod cli;
mod repl;
mod tui;

use clap::{Parser, Subcommand};
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::SyncEngine;
use ferrisync_core::DeviceInfo;
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
    /// Start the interactive shell (default when no command is given)
    Repl,
    /// Start the full-screen terminal UI
    Tui,
    /// Pair with a device by IP address
    Pair {
        ip: String,
        #[arg(long, default_value_t = cli::DEFAULT_PORT)]
        port: u16,
    },
    /// One-shot folder sync
    Sync {
        folder: String,
        #[arg(long)]
        device: String,
    },
    /// Show pairing and sync status
    Status,
    /// Watch a folder and sync on every change
    Watch {
        folder: String,
        #[arg(long)]
        device: String,
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
    let crypto = CryptoProvider::generate()?;
    Ok(Arc::new(crypto))
}

fn load_or_create_storage(data: &PathBuf) -> anyhow::Result<Arc<Storage>> {
    std::fs::create_dir_all(data)?;
    Ok(Arc::new(Storage::open(&data.join("metadata.db"))?))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
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

    match cli.command {
        Some(Commands::Tui) => {
            let engine = Arc::new(SyncEngine::new(
                storage.clone(),
                crypto.clone(),
                device_info.clone(),
            ));
            let pairing = PairingManager::new(crypto.clone(), storage.clone(), device_info.clone());
            tui::run_tui(engine, pairing, storage, device_info, &data).await?;
        }
        // Default: interactive shell
        Some(Commands::Repl) | None => {
            let pairing = PairingManager::new(crypto.clone(), storage.clone(), device_info.clone());
            repl::run(pairing, storage, crypto, device_info, &data).await?;
        }
        Some(Commands::Pair { ip, port }) => {
            let pairing = PairingManager::new(crypto.clone(), storage.clone(), device_info.clone());
            cli::pair::run(ip, port, &pairing).await?;
        }
        Some(Commands::Sync { folder, device }) => {
            cli::sync::run(folder, device, storage, crypto).await?;
        }
        Some(Commands::Status) => {
            cli::status::run(storage, device_info).await?;
        }
        Some(Commands::Watch { folder, device }) => {
            cli::watch::run(folder, device, storage, crypto).await?;
        }
    }

    Ok(())
}

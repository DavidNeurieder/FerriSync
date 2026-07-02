mod cli;
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
    /// Start the interactive terminal UI
    Tui,
    /// Pair with a device by IP address
    Pair {
        ip: String,
        #[arg(long, default_value = "9847")]
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
    /// Watch a folder for changes
    Watch {
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
    let crypto = CryptoProvider::generate()?;
    Ok(Arc::new(crypto))
}

fn load_or_create_storage(data: &PathBuf) -> anyhow::Result<Arc<Storage>> {
    std::fs::create_dir_all(data)?;
    Ok(Arc::new(Storage::open(&data.join("metadata.db"))?))
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

    let engine = Arc::new(SyncEngine::new(storage.clone(), crypto.clone(), device_info.clone()));
    let pairing = PairingManager::new(crypto.clone(), storage.clone(), device_info.clone());

    match cli.command {
        Some(cmd) => match cmd {
            Commands::Tui => {
                tui::run_tui(engine, pairing, storage, device_info, &data).await?;
            }
            Commands::Pair { ip, port } => {
                cli::pair::run(ip, port, &pairing).await?;
            }
            Commands::Sync { folder, device } => {
                cli::sync::run(folder, device, storage, crypto.clone()).await?;
            }
            Commands::Status => {
                cli::status::run(storage, device_info).await?;
            }
            Commands::Watch { folder } => {
                cli::watch::run(folder, String::new(), storage, crypto.clone()).await?;
            }
        },
        None => {
            // Default: launch TUI
            tui::run_tui(engine, pairing, storage, device_info, &data).await?;
        }
    }

    Ok(())
}

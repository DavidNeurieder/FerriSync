mod cli;
mod repl;
mod tui;

use clap::{Parser, Subcommand};
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::SyncEngine;
use ferrisync_core::DeviceInfo;
use std::path::{Path, PathBuf};
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
    /// One-shot folder sync (no args: sync all configured folders)
    Sync {
        /// Local folder path
        #[arg(requires = "device")]
        folder: Option<String>,
        /// Target device ID
        #[arg(long, requires = "folder")]
        device: Option<String>,
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

/// Stable per-data-dir identity: the TLS keypair is persisted so paired
/// devices recognize us across restarts; the device id is derived from the
/// certificate fingerprint.
async fn load_or_create_crypto(data: &Path) -> anyhow::Result<Arc<CryptoProvider>> {
    std::fs::create_dir_all(data)?;
    let cert_path = data.join("identity.crt");
    let key_path = data.join("identity.key");
    if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        let fingerprint = blake3::hash(&cert).as_bytes().to_vec();
        return Ok(Arc::new(CryptoProvider::load(cert, key, fingerprint)?));
    }
    let crypto = CryptoProvider::generate()?;
    let cert = crypto.certificate().await;
    std::fs::write(&cert_path, cert.as_ref())?;
    std::fs::write(&key_path, crypto.private_key().await.secret_der())?;
    Ok(Arc::new(crypto))
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
    Ok(Arc::new(Storage::open(&data.join("metadata.db"))?))
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
            cli::sync::run_dispatch(folder, device, storage, crypto).await?;
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

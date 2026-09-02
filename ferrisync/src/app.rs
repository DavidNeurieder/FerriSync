use anyhow::Context;
use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::pairing::PairingManager;
use ferrisync_core::sync_engine::SyncEngine;
use ferrisync_core::DeviceInfo;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Shared application state constructed once per invocation and handed to
/// whichever presentation layer the user selected (REPL or CLI command).
pub struct ApplicationContext {
    pub data_dir: PathBuf,
    pub crypto: Arc<CryptoProvider>,
    pub storage: Arc<Storage>,
    pub device_info: DeviceInfo,
    pub engine: Arc<SyncEngine>,
    pub pairing: PairingManager,
}

impl ApplicationContext {
    pub async fn new(data_dir_arg: String) -> anyhow::Result<Self> {
        let data_dir = if data_dir_arg.is_empty() {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("ferrisync")
        } else {
            PathBuf::from(&data_dir_arg)
        };

        let crypto = load_or_create_crypto(&data_dir).await?;
        let storage = load_or_create_storage(&data_dir)?;

        let cert_fingerprint = crypto.fingerprint().await;
        let dev_id = device_id_from_fingerprint(&cert_fingerprint);
        let device_info = DeviceInfo {
            id: dev_id,
            name: ferrisync_core::config::load_device_name(&data_dir).unwrap_or_else(|| {
                whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string())
            }),
            cert_fingerprint,
        };

        let state_store = Arc::new(InMemoryStateStore::new());
        let engine = Arc::new(SyncEngine::new(
            storage.clone(),
            crypto.clone(),
            device_info.clone(),
            state_store,
        ));
        let pairing = PairingManager::new(crypto.clone(), storage.clone(), device_info.clone());

        Ok(ApplicationContext {
            data_dir,
            crypto,
            storage,
            device_info,
            engine,
            pairing,
        })
    }

    /// Restore the device to a fresh-install state by wiping all persisted
    /// state in the data directory (identity, database, device-name config).
    /// User files are never touched. The next `ApplicationContext::new` run
    /// regenerates a brand-new identity.
    ///
    /// Callers running live servers/watches should tear those down first via
    /// their own state (e.g. `ReplState::stop_all`).
    pub async fn reset(&self) -> anyhow::Result<()> {
        std::fs::remove_dir_all(&self.data_dir)?;
        Ok(())
    }
}

/// Stable per-data-dir identity: the TLS keypair is persisted so paired
/// devices recognize us across restarts; the device id is derived from the
/// certificate fingerprint.
///
/// The canonical storage names are `identity.crt`/`identity.key`. A legacy
/// `cert.der`/`key.der` pair (written by the old `ferrisync-cli`) is migrated
/// into the canonical names so the certificate — and therefore the
/// fingerprint-derived device id — is preserved.
async fn load_or_create_crypto(data: &Path) -> anyhow::Result<Arc<CryptoProvider>> {
    std::fs::create_dir_all(data).with_context(|| format!("create data dir {}", data.display()))?;
    let cert_path = data.join("identity.crt");
    let key_path = data.join("identity.key");

    if cert_path.exists() && key_path.exists() {
        return load_crypto(&cert_path, &key_path);
    }

    let legacy_cert = data.join("cert.der");
    let legacy_key = data.join("key.der");
    if legacy_cert.exists() && legacy_key.exists() {
        std::fs::rename(&legacy_cert, &cert_path)?;
        std::fs::rename(&legacy_key, &key_path)?;
        return load_crypto(&cert_path, &key_path);
    }

    let crypto = CryptoProvider::generate()?;
    let cert = crypto.certificate().await;
    std::fs::write(&cert_path, cert.as_ref())?;
    std::fs::write(&key_path, crypto.private_key().await.secret_der())?;
    Ok(Arc::new(crypto))
}

fn load_crypto(cert_path: &Path, key_path: &Path) -> anyhow::Result<Arc<CryptoProvider>> {
    let cert = std::fs::read(cert_path)?;
    let key = std::fs::read(key_path)?;
    let fingerprint = blake3::hash(&cert).as_bytes().to_vec();
    Ok(Arc::new(CryptoProvider::load(cert, key, fingerprint)?))
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

fn load_or_create_storage(data: &Path) -> anyhow::Result<Arc<Storage>> {
    std::fs::create_dir_all(data).with_context(|| format!("create data dir {}", data.display()))?;
    Ok(Arc::new(Storage::open(&data.join("metadata.db"))?))
}

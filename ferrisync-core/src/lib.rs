pub mod api;
pub mod authorization;
pub mod config;
pub mod crypto;
pub mod discovery;
pub mod domain;
mod frb_generated;
pub mod filesystem;
pub mod path_safety;
pub mod persistence;
pub mod protocol;
pub mod protocol_v2;
pub mod storage;
pub mod sync;
pub mod sync_engine;
pub mod transfer;
pub mod transport;
pub mod watcher;

pub use crypto::CryptoProvider;
pub use discovery::DiscoveryService;
pub use protocol::SyncMessage;
pub use storage::Storage;
pub use sync_engine::pairing::PairingManager;
pub use sync_engine::SyncEngine;
pub use sync_engine::SyncEvent;
pub use transport::tcp::TcpTransport;

pub type DeviceId = String;
pub type Timestamp = i64;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    pub id: DeviceId,
    pub name: String,
    pub cert_fingerprint: Vec<u8>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncConfig {
    pub listen_port: u16,
    pub storage_path: String,
    pub device_name: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            listen_port: 9847,
            storage_path: ".ferrisync".to_string(),
            device_name: whoami::fallible::hostname().unwrap_or_else(|_| "ferrisync".to_string()),
        }
    }
}

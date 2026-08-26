// ── Public modules (part of the library's API surface) ──────────────
pub mod config;
pub mod crypto;
pub mod discovery;
pub mod domain;
pub mod storage;
pub mod sync_engine;
pub mod watcher;

// ── Internal modules (implementation details) ──────────────────────
// These modules are public so that downstream crates (CLI, TUI) can
// reach specific items when needed, but they are NOT part of the
// stable public API.  Prefer the re-exports below.
pub mod api;
pub mod authorization;
pub mod filesystem;
pub mod path_safety;
pub mod persistence;
pub mod protocol;
pub mod protocol_v2;
pub mod sync;
pub mod transfer;
pub mod transport;

// Flutter-Rust-Bridge glue — only used by the FFI codegen.
mod frb_generated;

// ── Public re-exports ──────────────────────────────────────────────
//
// The guiding rule (from the refactor plan):
//   "An external Rust developer should be able to use FerriSync
//    without knowing how FerriSync works internally."

// Core engine
pub use sync_engine::SyncEngine;
pub use sync_engine::SyncEvent;
pub use sync_engine::session::SyncResult;

// Identity & configuration
pub use crypto::CryptoProvider;
pub use storage::Storage;
pub use sync_engine::pairing::PairingManager;

// Server / host mode
pub use sync_engine::server::PairPolicy;
pub use sync_engine::server::ServeHandle;

// Bulk sync
pub use sync_engine::bulk::FolderOutcome;

// Single-folder sync (low-level — prefer SyncEngine::run_sync)
pub use sync_engine::session::run_sync_session;

// Discovery
pub use discovery::{DiscoveredPeer, DiscoveryService};

// Filesystem watching
pub use watcher::{ChangeScheduler, ChangeEvent, FileWatcher, SyncTrigger};

// Configuration helpers
pub use config::{load_device_name, persist_device_name};

// FFI helper (used by CLI/TUI for input validation)
pub use api::sanitize_device_name;

// Domain types — the shared vocabulary of the sync engine.
pub use domain::{
    Conflict, ConflictResolution, Device, Folder, FolderId, SyncDirection,
    FileHash, FilePath, FileVersion, Snapshot, SnapshotEntry, SyncPlan, SyncOperation,
};

// ── Top-level types ────────────────────────────────────────────────

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

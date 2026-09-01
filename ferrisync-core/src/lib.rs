//! # Ferrisync Core
//!
//! A reusable, peer-to-peer file synchronization engine for local networks.
//!
//! `ferrisync-core` provides encrypted, authenticated folder synchronization
//! between devices over LAN, with no central server or cloud service required.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use ferrisync_core::{CryptoProvider, Storage, SyncEngine, DeviceInfo};
//! use ferrisync_core::persistence::InMemoryStateStore;
//! use std::path::PathBuf;
//! use std::sync::Arc;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! // Generate device identity
//! let crypto = Arc::new(CryptoProvider::generate()?);
//! let fingerprint = crypto.fingerprint().await;
//! let storage = Arc::new(Storage::open(&PathBuf::from("metadata.db"))?);
//! let device_info = DeviceInfo {
//!     id: "device-uuid".into(),
//!     name: "my-device".into(),
//!     cert_fingerprint: fingerprint,
//! };
//!
//! // Create the sync engine
//! let engine = SyncEngine::new(
//!     storage,
//!     crypto,
//!     device_info,
//!     Arc::new(InMemoryStateStore::new()),
//! );
//!
//! // Sync a folder (see examples/minimal_sync.rs for a complete example)
//! // engine.run_sync("/path/to/folder", peer_addr, folder_id, &peer_id).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Architecture
//!
//! The library is organized into public modules (stable API surface) and
//! internal modules (implementation details):
//!
//! - [`sync_engine`] — the main `SyncEngine` orchestrator, session management,
//!   pairing, server hosting, and bulk sync
//! - [`crypto`] — TLS certificate generation and management
//! - [`storage`] — SQLite persistence for devices, folders, and file metadata
//! - [`domain`] — domain types: snapshots, conflicts, file versions, sync plans
//! - [`discovery`] — mDNS/DNS-SD LAN peer discovery
//! - [`watcher`] — filesystem watching with change debouncing
//! - [`config`] — device name configuration helpers
//!
//! ## Security
//!
//! See [`SECURITY.md`](https://github.com/DavidNeurieder/FerriSync/blob/main/SECURITY.md)
//! for the full security model and known limitations.
//!
//! Key security properties:
//! - TLS 1.3 on every connection (mutual authentication)
//! - Trust On First Use (TOFU) certificate pinning
//! - BLAKE3 hash verification before file commit
//! - Atomic writes via temp-file + rename
//! - Path traversal protection and input validation

// ── Public modules (part of the library's API surface) ──────────────
pub mod config;
pub mod crypto;
pub mod diagnostics;
pub mod discovery;
pub mod domain;
pub mod health;
pub mod storage;
pub mod sync_engine;
pub mod watcher;

// ── Internal modules (implementation details) ──────────────────────
// These modules are public so that downstream crates (CLI, Flutter) can
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
pub use sync_engine::session::SyncResult;
pub use sync_engine::SyncEngine;
pub use sync_engine::SyncEvent;

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
pub use watcher::{ChangeEvent, ChangeScheduler, FileWatcher, SyncTrigger};

// Configuration helpers
pub use config::{load_device_name, persist_device_name};

// FFI helper (used by CLI/REPL for input validation)
pub use api::sanitize_device_name;

// Domain types — the shared vocabulary of the sync engine.
pub use domain::{
    Conflict, ConflictResolution, Device, FileHash, FilePath, FileVersion, Folder, FolderId,
    Snapshot, SnapshotEntry, SyncDirection, SyncOperation, SyncPlan,
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

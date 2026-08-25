use anyhow::Result;
use async_trait::async_trait;

use crate::domain::{
    CertificateFingerprint, Device, DeviceId, FileMetadata, FilePath, FolderId, Tombstone,
};

/// A finished sync session record.
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub ts: i64,
    pub direction: String,
    pub peer_device: String,
    pub addr: String,
    pub folder_path: String,
    pub pushed_count: usize,
    pub pulled_count: usize,
    pub conflicts_count: usize,
}

/// An entry in the file history log.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub folder_id: FolderId,
    pub path: FilePath,
    pub device_id: DeviceId,
    pub action: String,
    pub version: i64,
    pub mtime: i64,
    pub hash: Vec<u8>,
    pub size: i64,
}

/// Summary of what a device removal deleted.
#[derive(Debug, Clone)]
pub struct DeviceCleanup {
    pub sessions_removed: usize,
    pub history_removed: usize,
    pub metadata_removed: usize,
    pub folders_removed: usize,
    pub device_removed: usize,
}

/// Database-agnostic persistence interface.
///
/// All sync state flows through this trait. Implementations can be SQLite,
/// in-memory, or any other backend. The domain layer depends only on this
/// trait — never on concrete database types.
#[async_trait]
pub trait StateStore: Send + Sync {
    // ── Device operations ──

    async fn get_device(&self, id: &DeviceId) -> Result<Option<Device>>;
    async fn upsert_device(&self, device: &Device) -> Result<()>;
    async fn list_devices(&self) -> Result<Vec<Device>>;
    async fn remove_device(&self, id: &DeviceId) -> Result<DeviceCleanup>;

    async fn get_device_cert(&self, id: &DeviceId) -> Result<Option<Vec<u8>>>;
    async fn set_device_cert(&self, id: &DeviceId, cert_der: &[u8]) -> Result<()>;
    async fn get_device_by_cert_fingerprint(
        &self,
        fingerprint: &CertificateFingerprint,
    ) -> Result<Option<DeviceId>>;
    async fn device_last_addr(&self, id: &DeviceId) -> Result<Option<String>>;
    async fn set_device_last_addr(&self, id: &DeviceId, addr: &str) -> Result<()>;

    // ── Folder operations ──

    async fn get_folder(&self, id: FolderId) -> Result<Option<crate::domain::Folder>>;
    async fn list_folders(&self) -> Result<Vec<crate::domain::Folder>>;
    async fn add_folder(
        &self,
        local_path: &str,
        device_id: &DeviceId,
        direction: &str,
    ) -> Result<FolderId>;
    async fn remove_folder(&self, id: FolderId) -> Result<()>;
    async fn set_folder_last_sync(&self, id: FolderId, ts: i64) -> Result<()>;
    async fn set_folder_device(&self, id: FolderId, device_id: &DeviceId) -> Result<()>;

    // ── File metadata ──

    async fn get_file_metadata(
        &self,
        folder_id: FolderId,
        path: &FilePath,
    ) -> Result<Option<FileMetadata>>;
    async fn upsert_file_metadata(&self, meta: &FileMetadata) -> Result<()>;

    // ── History ──

    async fn record_history(&self, entry: &HistoryEntry) -> Result<()>;

    // ── Tombstones ──

    async fn get_tombstones(&self, folder_id: FolderId, since: u64) -> Result<Vec<Tombstone>>;
    async fn add_tombstone(&self, tomb: &Tombstone) -> Result<()>;

    // ── Sessions ──

    async fn record_session(&self, session: &SessionRecord) -> Result<()>;
    async fn list_recent_sessions(&self, limit: u32) -> Result<Vec<SessionRecord>>;

    // ── Bulk operations ──

    async fn clear_all_sync_state(&self) -> Result<(usize, usize)>;
    async fn remove_sync_folders(
        &self,
        local_path: &str,
        device_id: Option<&DeviceId>,
    ) -> Result<usize>;
}

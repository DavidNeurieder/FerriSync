use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::file::{FilePath, FileHash, FileVersion};
use crate::domain::folder::FolderId;
use crate::domain::snapshot::{Snapshot, SnapshotEntry};
use crate::domain::sync_plan::SyncOperation;
use crate::domain::tombstone::Tombstone;
use crate::filesystem::SyncRoot;
use crate::persistence::StateStore;
use crate::protocol::{
    FileChunk, Index, IndexEntry, MAX_CHUNK_FRAME, MAX_FILE_SIZE, MAX_PATH_LEN,
};
use crate::sync::reconciler::reconcile;
use crate::sync::snapshot::SnapshotBuilder;

/// Result of a sync session executed by the orchestrator.
#[derive(Debug, Default, Clone)]
pub struct OrchestratorResult {
    pub pushed: Vec<String>,
    pub pulled: Vec<String>,
    pub conflicts: Vec<String>,
}

/// The sync orchestrator coordinates all synchronization activities.
///
/// It replaces the inline logic in `session.rs` with calls through the
/// new module architecture:
///
/// ```text
/// Authorization (IdentityVerifier)
///       ↓
/// SnapshotBuilder (local filesystem scan)
///       ↓
/// reconcile() (pure function → SyncPlan)
///       ↓
/// TransferManager (atomic file I/O via SyncRoot)
/// ```
///
/// The orchestrator does NOT handle TLS or framing — those remain in
/// the transport layer. It works with abstract `SyncPlan` operations
/// and `SyncRoot` file access.
pub struct SyncOrchestrator {
    root: Arc<SyncRoot>,
    store: Arc<dyn StateStore>,
    folder_id: FolderId,
}

impl SyncOrchestrator {
    pub fn new(root: Arc<SyncRoot>, store: Arc<dyn StateStore>, folder_id: FolderId) -> Self {
        Self {
            root,
            store,
            folder_id,
        }
    }

    /// Build a local snapshot from the filesystem.
    pub fn build_local_snapshot(&self, generation: u64) -> Result<Snapshot> {
        SnapshotBuilder::new(self.root.clone()).build(self.folder_id, generation)
    }

    /// Convert a protocol `Index` into a domain `Snapshot`.
    pub fn index_to_snapshot(index: &Index, folder_id: FolderId) -> Snapshot {
        let entries: Vec<SnapshotEntry> = index
            .entries
            .iter()
            .map(|e| SnapshotEntry {
                path: FilePath(e.path.clone()),
                kind: crate::domain::file::EntryKind::File,
                size: e.size,
                hash: {
                    let mut h = [0u8; 32];
                    let len = e.hash.len().min(32);
                    h[..len].copy_from_slice(&e.hash[..len]);
                    FileHash(h)
                },
                version: FileVersion::new(),
                mtime: e.mtime,
            })
            .collect();
        Snapshot {
            folder_id,
            generation: 0,
            entries,
        }
    }

    /// Convert a domain `Snapshot` into a protocol `Index`.
    pub fn snapshot_to_index(snapshot: &Snapshot, folder_id_str: &str) -> Index {
        Index {
            folder_id: folder_id_str.to_string(),
            entries: snapshot
                .entries
                .iter()
                .map(|e| IndexEntry {
                    path: e.path.0.clone(),
                    local_version: e.mtime as u64,
                    remote_version: 0,
                    mtime: e.mtime,
                    size: e.size,
                    hash: e.hash.0.to_vec(),
                })
                .collect(),
        }
    }

    /// Run reconciliation between a local and remote snapshot.
    ///
    /// This is a pure function — no I/O.
    pub async fn reconcile(
        &self,
        local: &Snapshot,
        remote: &Snapshot,
    ) -> Result<crate::domain::SyncPlan> {
        let local_tombs = self
            .store
            .get_tombstones(self.folder_id, 0)
            .await
            .unwrap_or_default();
        let remote_tombs: Vec<Tombstone> = Vec::new(); // received from remote in future
        Ok(reconcile(local, remote, &local_tombs, &remote_tombs))
    }

    /// Execute upload operations: read files and produce FileChunk messages.
    pub fn plan_uploads(&self, ops: &[SyncOperation]) -> Vec<(FilePath, Vec<u8>)> {
        let mut results = Vec::new();
        for op in ops {
            if let SyncOperation::Upload { path, .. } = op {
                match self.read_file(path) {
                    Ok(data) => results.push((path.clone(), data)),
                    Err(e) => {
                        log::warn!("read failed for {}: {e}", path.as_str());
                    }
                }
            }
        }
        results
    }

    /// Read a file from the local filesystem through SyncRoot.
    pub fn read_file(&self, path: &FilePath) -> Result<Vec<u8>> {
        let full = self.root.safe_join(path.as_str())?;
        std::fs::read(&full).with_context(|| format!("read failed: {}", path.as_str()))
    }

    /// Write a received file to the local filesystem atomically.
    pub fn write_file(&self, path: &FilePath, data: &[u8]) -> Result<()> {
        let target = self.root.safe_join(path.as_str())?;

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create dirs failed: {}", parent.display()))?;
        }

        // Atomic write: temp file + rename
        let temp_name = format!(
            ".ferrisync-tmp-{}-{}",
            path.as_str().replace('/', "_"),
            std::process::id()
        );
        let temp_path = self.root.root().join(".ferrisync-tmp").join(&temp_name);

        if let Some(parent) = temp_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&temp_path, data)
            .with_context(|| format!("temp write failed: {}", temp_path.display()))?;
        std::fs::rename(&temp_path, &target).with_context(|| {
            let _ = std::fs::remove_file(&temp_path);
            format!(
                "atomic rename failed: {} -> {}",
                temp_path.display(),
                target.display()
            )
        })?;

        Ok(())
    }
}

// ── Protocol conversion helpers ──

/// Build a protocol `Index` by scanning a directory.
/// This is the bridge between old `build_index` and new `SnapshotBuilder`.
pub fn build_protocol_index(root: PathBuf, folder_id: i64) -> Result<Vec<IndexEntry>> {
    let sync_root = SyncRoot::open(root)?;
    let snap = SnapshotBuilder::new(Arc::new(sync_root)).build(FolderId(folder_id), 0)?;
    let idx = SyncOrchestrator::snapshot_to_index(&snap, &folder_id.to_string());
    Ok(idx.entries)
}

/// Validate an incoming `FileChunk` before buffering it.
pub fn validate_chunk(
    chunk: &FileChunk,
    in_flight: usize,
    buffered_bytes: usize,
) -> Result<()> {
    if chunk.path.len() > MAX_PATH_LEN {
        bail!(
            "file path too long: {} bytes (max {})",
            chunk.path.len(),
            MAX_PATH_LEN
        );
    }
    if chunk.total_size > MAX_FILE_SIZE {
        bail!(
            "file too large: {} bytes (max {})",
            chunk.total_size,
            MAX_FILE_SIZE
        );
    }
    if chunk.data.len() > MAX_CHUNK_FRAME {
        bail!(
            "chunk too large: {} bytes (max {})",
            chunk.data.len(),
            MAX_CHUNK_FRAME
        );
    }
    let end = chunk.offset
        .checked_add(chunk.data.len() as u64)
        .context("chunk offset + size overflow")?;
    if end > chunk.total_size {
        bail!(
            "chunk extends past total_size: offset={}, len={}, total={}",
            chunk.offset,
            chunk.data.len(),
            chunk.total_size
        );
    }
    if in_flight >= 64 {
        bail!("too many concurrent in-flight files: {in_flight} (max 64)");
    }
    if buffered_bytes + chunk.data.len() > 256 * 1024 * 1024 {
        bail!(
            "buffered bytes limit exceeded: {} + {} > 256 MiB",
            buffered_bytes,
            chunk.data.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::device::DeviceId;
    use crate::domain::file::EntryKind;
    use crate::persistence::InMemoryStateStore;

    fn setup() -> (tempfile::TempDir, Arc<SyncRoot>, Arc<InMemoryStateStore>, SyncOrchestrator) {
        let dir = tempfile::tempdir().unwrap();
        let root = Arc::new(SyncRoot::open(dir.path().to_path_buf()).unwrap());
        let store = Arc::new(InMemoryStateStore::new());
        let folder_id = FolderId(1);
        let orch = SyncOrchestrator::new(root.clone(), store.clone(), folder_id);
        (dir, root, store, orch)
    }

    #[test]
    fn build_local_snapshot_finds_files() {
        let (dir, _root, _store, orch) = setup();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("b.txt"), "world").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/c.txt"), "nested").unwrap();

        let snap = orch.build_local_snapshot(1).unwrap();
        assert_eq!(snap.entries.len(), 3);
        assert!(snap.find(&FilePath("a.txt".into())).is_some());
        assert!(snap.find(&FilePath("b.txt".into())).is_some());
        assert!(snap.find(&FilePath("sub/c.txt".into())).is_some());
    }

    #[test]
    fn index_to_snapshot_roundtrip() {
        let index = Index {
            folder_id: "1".into(),
            entries: vec![
                IndexEntry {
                    path: "x.txt".into(),
                    local_version: 100,
                    remote_version: 0,
                    mtime: 2000,
                    size: 42,
                    hash: blake3::hash(b"data").as_bytes().to_vec(),
                },
                IndexEntry {
                    path: "y.txt".into(),
                    local_version: 200,
                    remote_version: 0,
                    mtime: 3000,
                    size: 99,
                    hash: blake3::hash(b"other").as_bytes().to_vec(),
                },
            ],
        };

        let snap = SyncOrchestrator::index_to_snapshot(&index, FolderId(1));
        assert_eq!(snap.entries.len(), 2);
        assert_eq!(snap.entries[0].path, FilePath("x.txt".into()));
        assert_eq!(snap.entries[0].size, 42);
        assert_eq!(snap.entries[1].path, FilePath("y.txt".into()));
        assert_eq!(snap.entries[1].size, 99);

        // Roundtrip back to index
        let idx = SyncOrchestrator::snapshot_to_index(&snap, "1");
        assert_eq!(idx.folder_id, "1");
        assert_eq!(idx.entries.len(), 2);
        assert_eq!(idx.entries[0].path, "x.txt");
        assert_eq!(idx.entries[1].path, "y.txt");
    }

    #[tokio::test]
    async fn reconcile_local_only_uploads() {
        let (dir, _root, _store, orch) = setup();
        std::fs::write(dir.path().join("new.txt"), "data").unwrap();

        let local = orch.build_local_snapshot(1).unwrap();
        let remote = Snapshot::new(FolderId(1), 0);

        let plan = orch.reconcile(&local, &remote).await.unwrap();
        assert_eq!(plan.uploads.len(), 1);
        assert_eq!(plan.uploads[0].path().as_str(), "new.txt");
    }

    #[tokio::test]
    async fn reconcile_remote_only_downloads() {
        let (dir, _root, _store, orch) = setup();
        let local = orch.build_local_snapshot(1).unwrap();

        let remote = Snapshot {
            folder_id: FolderId(1),
            generation: 0,
            entries: vec![SnapshotEntry {
                path: FilePath("remote.txt".into()),
                kind: EntryKind::File,
                size: 50,
                hash: FileHash::from_bytes(b"remote data"),
                version: FileVersion::single(DeviceId("dev-remote".into()), 1),
                mtime: 5000,
            }],
        };

        let plan = orch.reconcile(&local, &remote).await.unwrap();
        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(plan.downloads[0].path().as_str(), "remote.txt");
    }

    #[tokio::test]
    async fn reconcile_identical_empty_plan() {
        let (dir, _root, _store, orch) = setup();
        std::fs::write(dir.path().join("same.txt"), "content").unwrap();

        let local = orch.build_local_snapshot(1).unwrap();
        // Build a remote snapshot with the same hash
        let remote_entry = SnapshotEntry {
            path: FilePath("same.txt".into()),
            kind: EntryKind::File,
            size: 7,
            hash: local.entries[0].hash.clone(),
            version: FileVersion::new(),
            mtime: local.entries[0].mtime,
        };
        let remote = Snapshot {
            folder_id: FolderId(1),
            generation: 0,
            entries: vec![remote_entry],
        };

        let plan = orch.reconcile(&local, &remote).await.unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn read_write_file_through_orchestrator() {
        let (_dir, _root, _store, orch) = setup();
        orch.write_file(&FilePath("test.txt".into()), b"hello")
            .unwrap();
        let data = orch.read_file(&FilePath("test.txt".into())).unwrap();
        assert_eq!(data, b"hello");
    }

    #[test]
    fn plan_uploads_reads_files() {
        let (dir, _root, _store, orch) = setup();
        std::fs::write(dir.path().join("a.txt"), b"content_a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"content_b").unwrap();

        let ops = vec![
            SyncOperation::Upload {
                path: FilePath("a.txt".into()),
                version: FileVersion::new(),
                size: 9,
            },
            SyncOperation::Download {
                path: FilePath("b.txt".into()),
                version: FileVersion::new(),
                size: 9,
            },
        ];

        let results = orch.plan_uploads(&ops);
        assert_eq!(results.len(), 1); // only Upload ops
        assert_eq!(results[0].0, FilePath("a.txt".into()));
        assert_eq!(results[0].1, b"content_a");
    }

    #[test]
    fn validate_chunk_rejects_long_path() {
        let chunk = FileChunk {
            folder_id: "1".into(),
            path: "x".repeat(MAX_PATH_LEN + 1),
            offset: 0,
            data: vec![],
            total_size: 100,
        };
        assert!(validate_chunk(&chunk, 0, 0).is_err());
    }

    #[test]
    fn validate_chunk_rejects_oversized_file() {
        let chunk = FileChunk {
            folder_id: "1".into(),
            path: "big.bin".into(),
            offset: 0,
            data: vec![],
            total_size: MAX_FILE_SIZE + 1,
        };
        assert!(validate_chunk(&chunk, 0, 0).is_err());
    }

    #[test]
    fn validate_chunk_rejects_offset_overflow() {
        let chunk = FileChunk {
            folder_id: "1".into(),
            path: "f.txt".into(),
            offset: u64::MAX,
            data: vec![1, 2, 3],
            total_size: 100,
        };
        assert!(validate_chunk(&chunk, 0, 0).is_err());
    }

    #[test]
    fn validate_chunk_rejects_too_many_in_flight() {
        let chunk = FileChunk {
            folder_id: "1".into(),
            path: "f.txt".into(),
            offset: 0,
            data: vec![],
            total_size: 100,
        };
        assert!(validate_chunk(&chunk, 65, 0).is_err());
    }

    #[test]
    fn validate_chunk_rejects_buffer_overflow() {
        let chunk = FileChunk {
            folder_id: "1".into(),
            path: "f.txt".into(),
            offset: 0,
            data: vec![0u8; 1024],
            total_size: 100,
        };
        // Already have 256MiB buffered + 1KB chunk
        assert!(validate_chunk(&chunk, 1, 256 * 1024 * 1024).is_err());
    }
}

use anyhow::Result;
use std::sync::Arc;

use crate::domain::folder::FolderId;
use crate::domain::snapshot::{Snapshot, SnapshotEntry};
use crate::filesystem::SyncRoot;

/// Builds local snapshots from the filesystem.
///
/// The snapshot is the reconciler's input. It captures a consistent view
/// of the folder contents at a point in time.
pub struct SnapshotBuilder {
    root: Arc<SyncRoot>,
}

impl SnapshotBuilder {
    pub fn new(root: Arc<SyncRoot>) -> Self {
        Self { root }
    }

    /// Build a snapshot of the local filesystem.
    ///
    /// Scans the directory tree, hashes every file, and produces a
    /// `Snapshot` suitable for reconciliation.
    pub fn build(&self, folder_id: FolderId, generation: u64) -> Result<Snapshot> {
        self.root.scan(folder_id, generation)
    }

    /// Build a snapshot from a remote index (network representation).
    ///
    /// Converts protocol-level index entries into domain `SnapshotEntry` values.
    pub fn from_remote_index(
        folder_id: FolderId,
        generation: u64,
        entries: Vec<crate::protocol::IndexEntry>,
    ) -> Snapshot {
        let entries: Vec<SnapshotEntry> = entries
            .into_iter()
            .map(|e| SnapshotEntry {
                path: crate::domain::FilePath(e.path),
                kind: crate::domain::file::EntryKind::File,
                size: e.size,
                hash: crate::domain::FileHash::from_bytes(&e.hash),
                version: crate::domain::FileVersion::new(), // remote versions tracked separately
                mtime: e.mtime,
            })
            .collect();
        Snapshot {
            folder_id,
            generation,
            entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::file::FilePath;

    #[test]
    fn from_remote_index_converts_entries() {
        let entries = vec![
            crate::protocol::IndexEntry {
                path: "a.txt".into(),
                local_version: 1,
                remote_version: 0,
                mtime: 1000,
                size: 42,
                hash: blake3::hash(b"hello").as_bytes().to_vec(),
            },
            crate::protocol::IndexEntry {
                path: "b.txt".into(),
                local_version: 0,
                remote_version: 0,
                mtime: 2000,
                size: 99,
                hash: blake3::hash(b"world").as_bytes().to_vec(),
            },
        ];
        let snap = SnapshotBuilder::from_remote_index(FolderId(1), 5, entries);
        assert_eq!(snap.folder_id.0, 1);
        assert_eq!(snap.generation, 5);
        assert_eq!(snap.entries.len(), 2);
        assert_eq!(snap.entries[0].path, FilePath("a.txt".into()));
        assert_eq!(snap.entries[0].size, 42);
        assert_eq!(snap.entries[1].path, FilePath("b.txt".into()));
        assert_eq!(snap.entries[1].size, 99);
    }
}

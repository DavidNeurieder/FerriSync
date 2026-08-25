use serde::{Deserialize, Serialize};

use super::file::{EntryKind, FileHash, FilePath, FileVersion};
use super::folder::FolderId;

/// A point-in-time description of every file in a folder.
///
/// Snapshots are the reconciler's input. They can originate from:
/// - the local filesystem (via `SnapshotBuilder`)
/// - the network (from a remote peer's `Index` message)
/// - an in-memory test fixture
///
/// The reconciler does not care about the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub folder_id: FolderId,
    pub generation: u64,
    pub entries: Vec<SnapshotEntry>,
}

/// A single entry in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotEntry {
    pub path: FilePath,
    pub kind: EntryKind,
    pub size: u64,
    pub hash: FileHash,
    pub version: FileVersion,
    pub mtime: i64,
}

impl Snapshot {
    pub fn new(folder_id: FolderId, generation: u64) -> Self {
        Self {
            folder_id,
            generation,
            entries: Vec::new(),
        }
    }

    /// Find an entry by path.
    pub fn find(&self, path: &FilePath) -> Option<&SnapshotEntry> {
        self.entries.iter().find(|e| &e.path == path)
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the snapshot is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::device::DeviceId;

    fn entry(path: &str, hash: &[u8]) -> SnapshotEntry {
        SnapshotEntry {
            path: FilePath(path.into()),
            kind: EntryKind::File,
            size: 100,
            hash: FileHash(*blake3::hash(hash).as_bytes()),
            version: FileVersion::single(DeviceId("dev-a".into()), 1),
            mtime: 1000,
        }
    }

    #[test]
    fn snapshot_find() {
        let mut snap = Snapshot::new(FolderId(1), 1);
        snap.entries.push(entry("a.txt", b"a"));
        snap.entries.push(entry("b.txt", b"b"));

        assert!(snap.find(&FilePath("a.txt".into())).is_some());
        assert!(snap.find(&FilePath("c.txt".into())).is_none());
    }

    #[test]
    fn snapshot_len() {
        let mut snap = Snapshot::new(FolderId(1), 1);
        assert!(snap.is_empty());
        snap.entries.push(entry("a.txt", b"a"));
        assert_eq!(snap.len(), 1);
        assert!(!snap.is_empty());
    }
}

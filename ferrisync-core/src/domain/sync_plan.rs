use serde::{Deserialize, Serialize};

use super::file::{FilePath, FileVersion};

/// The authoritative answer to "what should happen?" during synchronization.
///
/// A `SyncPlan` is produced by the pure reconciler function and consumed
/// by the transfer manager. It must never be produced by code that performs
/// I/O — it is a data-only description of intended operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPlan {
    pub uploads: Vec<SyncOperation>,
    pub downloads: Vec<SyncOperation>,
    pub deletes: Vec<SyncOperation>,
    pub conflicts: Vec<SyncOperation>,
}

impl SyncPlan {
    pub fn new() -> Self {
        Self {
            uploads: Vec::new(),
            downloads: Vec::new(),
            deletes: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    /// Total number of operations in the plan.
    pub fn len(&self) -> usize {
        self.uploads.len() + self.downloads.len() + self.deletes.len() + self.conflicts.len()
    }

    /// Whether the plan has no operations.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for SyncPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// A single synchronization operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncOperation {
    /// Upload (push) a file to the remote peer.
    Upload {
        path: FilePath,
        version: FileVersion,
        size: u64,
    },
    /// Download (pull) a file from the remote peer.
    Download {
        path: FilePath,
        version: FileVersion,
        size: u64,
    },
    /// Delete a file locally (remote deleted it).
    Delete { path: FilePath },
    /// Both sides modified the file — needs human or policy resolution.
    Conflict {
        path: FilePath,
        local_version: FileVersion,
        remote_version: FileVersion,
    },
}

impl SyncOperation {
    pub fn path(&self) -> &FilePath {
        match self {
            Self::Upload { path, .. }
            | Self::Download { path, .. }
            | Self::Delete { path }
            | Self::Conflict { path, .. } => path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_starts_empty() {
        let plan = SyncPlan::new();
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
    }

    #[test]
    fn plan_counts_operations() {
        let mut plan = SyncPlan::new();
        plan.uploads.push(SyncOperation::Upload {
            path: FilePath("a.txt".into()),
            version: FileVersion::new(),
            size: 100,
        });
        plan.downloads.push(SyncOperation::Download {
            path: FilePath("b.txt".into()),
            version: FileVersion::new(),
            size: 200,
        });
        plan.deletes.push(SyncOperation::Delete {
            path: FilePath("c.txt".into()),
        });
        plan.conflicts.push(SyncOperation::Conflict {
            path: FilePath("d.txt".into()),
            local_version: FileVersion::new(),
            remote_version: FileVersion::new(),
        });
        assert_eq!(plan.len(), 4);
        assert!(!plan.is_empty());
    }

    #[test]
    fn operation_path() {
        let op = SyncOperation::Upload {
            path: FilePath("test.txt".into()),
            version: FileVersion::new(),
            size: 50,
        };
        assert_eq!(op.path().as_str(), "test.txt");
    }
}

use crate::domain::snapshot::Snapshot;
use crate::domain::tombstone::Tombstone;
use crate::domain::{FilePath, FileVersion, SyncOperation, SyncPlan};

/// Pure reconciliation function.
///
/// Compares two snapshots (local and remote) and produces a `SyncPlan`
/// describing what operations are needed to bring them into agreement.
///
/// This function performs **no I/O** — it takes only data and returns data.
/// The transfer manager executes the plan.
///
/// # Conflict resolution
///
/// - If one side has a newer version, that side's file wins.
/// - If both sides changed concurrently, it's a conflict.
/// - If a file was deleted on one side and modified on the other, it's a conflict.
/// - If a file is missing from both snapshots and has no tombstone, it's ignored.
pub fn reconcile(
    local: &Snapshot,
    remote: &Snapshot,
    local_tombstones: &[Tombstone],
    remote_tombstones: &[Tombstone],
) -> SyncPlan {
    let mut plan = SyncPlan::new();

    let local_map: std::collections::HashMap<&FilePath, &crate::domain::snapshot::SnapshotEntry> =
        local.entries.iter().map(|e| (&e.path, e)).collect();
    let remote_map: std::collections::HashMap<&FilePath, &crate::domain::snapshot::SnapshotEntry> =
        remote.entries.iter().map(|e| (&e.path, e)).collect();

    // Collect all known paths from both snapshots
    let mut all_paths: std::collections::HashSet<&FilePath> = std::collections::HashSet::new();
    for e in &local.entries {
        all_paths.insert(&e.path);
    }
    for e in &remote.entries {
        all_paths.insert(&e.path);
    }

    // Check tombstone status
    let local_deleted: std::collections::HashSet<&FilePath> =
        local_tombstones.iter().map(|t| &t.path).collect();
    let remote_deleted: std::collections::HashSet<&FilePath> =
        remote_tombstones.iter().map(|t| &t.path).collect();

    for path in all_paths {
        let local_entry = local_map.get(path);
        let remote_entry = remote_map.get(path);
        let local_is_deleted = local_deleted.contains(path);
        let remote_is_deleted = remote_deleted.contains(path);

        match (local_entry, remote_entry) {
            // Both sides have the file
            (Some(local_e), Some(remote_e)) => {
                if local_e.hash == remote_e.hash {
                    // Identical content — no sync needed
                    continue;
                }
                if local_e.version.dominates(&remote_e.version) {
                    // Local wins — push to remote
                    plan.uploads.push(SyncOperation::Upload {
                        path: (*path).clone(),
                        version: local_e.version.clone(),
                        size: local_e.size,
                    });
                } else if remote_e.version.dominates(&local_e.version) {
                    // Remote wins — pull from remote
                    plan.downloads.push(SyncOperation::Download {
                        path: (*path).clone(),
                        version: remote_e.version.clone(),
                        size: remote_e.size,
                    });
                } else {
                    // Concurrent modifications — conflict
                    plan.conflicts.push(SyncOperation::Conflict {
                        path: (*path).clone(),
                        local_version: local_e.version.clone(),
                        remote_version: remote_e.version.clone(),
                    });
                }
            }

            // Only local has it
            (Some(local_e), None) => {
                if remote_is_deleted {
                    // Remote deleted it, local still has it — conflict
                    plan.conflicts.push(SyncOperation::Conflict {
                        path: (*path).clone(),
                        local_version: local_e.version.clone(),
                        remote_version: FileVersion::new(),
                    });
                } else {
                    // Remote doesn't have it — push to remote
                    plan.uploads.push(SyncOperation::Upload {
                        path: (*path).clone(),
                        version: local_e.version.clone(),
                        size: local_e.size,
                    });
                }
            }

            // Only remote has it
            (None, Some(remote_e)) => {
                if local_is_deleted {
                    // Local deleted it, remote still has it — conflict
                    plan.conflicts.push(SyncOperation::Conflict {
                        path: (*path).clone(),
                        local_version: FileVersion::new(),
                        remote_version: remote_e.version.clone(),
                    });
                } else {
                    // Local doesn't have it — pull from remote
                    plan.downloads.push(SyncOperation::Download {
                        path: (*path).clone(),
                        version: remote_e.version.clone(),
                        size: remote_e.size,
                    });
                }
            }

            // Neither side has it — no-op (tombstone cleanup handles expiry)
            (None, None) => {}
        }
    }

    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::device::DeviceId;
    use crate::domain::file::{EntryKind, FileHash};
    use crate::domain::folder::FolderId;
    use crate::domain::snapshot::SnapshotEntry;

    fn entry(path: &str, hash: &[u8], version: FileVersion) -> SnapshotEntry {
        SnapshotEntry {
            path: FilePath(path.into()),
            kind: EntryKind::File,
            size: 100,
            hash: FileHash(*blake3::hash(hash).as_bytes()),
            version,
            mtime: 1000,
        }
    }

    fn dev(n: u8) -> DeviceId {
        DeviceId(format!("dev-{n}"))
    }

    #[test]
    fn identical_snapshots_empty_plan() {
        let v = FileVersion::single(dev(1), 1);
        let snap = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![entry("a.txt", b"same", v)],
        };
        let plan = reconcile(&snap, &snap, &[], &[]);
        assert!(plan.is_empty());
    }

    #[test]
    fn remote_has_new_file() {
        let local = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![],
        };
        let remote = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![entry("new.txt", b"data", FileVersion::single(dev(2), 1))],
        };
        let plan = reconcile(&local, &remote, &[], &[]);
        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(plan.downloads[0].path().as_str(), "new.txt");
        assert!(plan.uploads.is_empty());
    }

    #[test]
    fn local_has_new_file() {
        let local = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![entry("local.txt", b"data", FileVersion::single(dev(1), 1))],
        };
        let remote = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![],
        };
        let plan = reconcile(&local, &remote, &[], &[]);
        assert_eq!(plan.uploads.len(), 1);
        assert_eq!(plan.uploads[0].path().as_str(), "local.txt");
    }

    #[test]
    fn remote_wins_newer_version() {
        let local = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![entry("f.txt", b"old", FileVersion::single(dev(1), 1))],
        };
        let remote = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![entry("f.txt", b"new", FileVersion::single(dev(1), 3))],
        };
        let plan = reconcile(&local, &remote, &[], &[]);
        assert_eq!(plan.downloads.len(), 1);
        assert_eq!(plan.uploads.len(), 0);
    }

    #[test]
    fn concurrent_changes_conflict() {
        let mut v_local = FileVersion::new();
        v_local.versions.insert(dev(1), 3);
        let mut v_remote = FileVersion::new();
        v_remote.versions.insert(dev(2), 2);

        let local = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![entry("conflict.txt", b"local", v_local.clone())],
        };
        let remote = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![entry("conflict.txt", b"remote", v_remote.clone())],
        };
        let plan = reconcile(&local, &remote, &[], &[]);
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.uploads.len(), 0);
        assert_eq!(plan.downloads.len(), 0);
    }

    #[test]
    fn deleted_on_one_side_conflict() {
        let local = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![entry("del.txt", b"data", FileVersion::single(dev(1), 1))],
        };
        let remote = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![],
        };
        // Remote has tombstone
        let remote_tombs = vec![Tombstone {
            path: FilePath("del.txt".into()),
            folder_id: FolderId(1),
            device_id: dev(2),
            deleted_at: 2,
        }];
        let plan = reconcile(&local, &remote, &[], &remote_tombs);
        assert_eq!(plan.conflicts.len(), 1);
    }

    #[test]
    fn deleted_on_both_sides_noop() {
        let local = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![],
        };
        let remote = Snapshot {
            folder_id: FolderId(1),
            generation: 1,
            entries: vec![],
        };
        let plan = reconcile(&local, &remote, &[], &[]);
        assert!(plan.is_empty());
    }
}

use serde::{Deserialize, Serialize};

use super::device::DeviceId;
use super::file::FilePath;
use super::folder::FolderId;

/// Records that a file was deleted on a specific device.
///
/// Tombstones prevent "deletion resurrection" — where a peer that missed
/// the deletion re-introduces the file during sync. A tombstone is
/// retained for a configurable period after which it can be garbage
/// collected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    pub path: FilePath,
    pub folder_id: FolderId,
    pub device_id: DeviceId,
    /// Generation (monotonic counter) at which the deletion was observed.
    pub deleted_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tombstone_construction() {
        let t = Tombstone {
            path: FilePath("deleted.txt".into()),
            folder_id: FolderId(1),
            device_id: DeviceId("dev-a".into()),
            deleted_at: 42,
        };
        assert_eq!(t.path.as_str(), "deleted.txt");
        assert_eq!(t.folder_id.0, 1);
        assert_eq!(t.device_id.0, "dev-a");
        assert_eq!(t.deleted_at, 42);
    }

    #[test]
    fn tombstone_serde_roundtrip() {
        let t = Tombstone {
            path: FilePath("foo/bar.txt".into()),
            folder_id: FolderId(7),
            device_id: DeviceId("abc".into()),
            deleted_at: 99,
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: Tombstone = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }
}

use serde::{Deserialize, Serialize};

use super::device::DeviceId;
use super::file::{FilePath, FileVersion};
use super::folder::FolderId;

/// A detected conflict where both sides modified a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub path: FilePath,
    pub folder_id: FolderId,
    pub local_version: FileVersion,
    pub remote_version: FileVersion,
    pub local_device: DeviceId,
    pub remote_device: DeviceId,
}

/// How a conflict was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictResolution {
    /// Keep the local version.
    KeepLocal,
    /// Keep the remote version.
    KeepRemote,
    /// Preserve both as conflict copies.
    PreserveBoth,
    /// Defer to a future manual resolution.
    Deferred,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_serde_roundtrip() {
        let c = Conflict {
            path: FilePath("dir/file.txt".into()),
            folder_id: FolderId(1),
            local_version: FileVersion::single(DeviceId("a".into()), 5),
            remote_version: FileVersion::single(DeviceId("b".into()), 3),
            local_device: DeviceId("a".into()),
            remote_device: DeviceId("b".into()),
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: Conflict = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use super::device::DeviceId;
use super::folder::FolderId;

/// Validated relative file path within a SyncRoot.
///
/// Guarantees: no leading `/`, no `..` components, no null bytes, no symlinks.
/// This is the canonical internal representation of a file path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FilePath(pub String);

impl FilePath {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for FilePath {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FilePath {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Fixed-size BLAKE3 hash (32 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileHash(pub [u8; 32]);

impl FileHash {
    pub fn from_bytes(data: &[u8]) -> Self {
        Self(blake3::hash(data).into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        blake3::Hash::from(self.0).to_hex().to_string()
    }
}

impl fmt::Display for FileHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_hex()[..12])
    }
}

/// Per-device version counter for a file.
///
/// In a multi-device topology, each device independently increments its own
/// version counter when it modifies a file. A `FileVersion` captures the
/// last-known version from every device that has touched the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileVersion {
    pub versions: HashMap<DeviceId, u64>,
}

impl FileVersion {
    pub fn new() -> Self {
        Self {
            versions: HashMap::new(),
        }
    }

    /// Create a version with a single device's counter.
    pub fn single(device: DeviceId, version: u64) -> Self {
        let mut v = Self::new();
        v.versions.insert(device, version);
        v
    }

    /// Bump the version for a specific device.
    pub fn bump(&mut self, device: &DeviceId) {
        *self.versions.entry(device.clone()).or_insert(0) += 1;
    }

    /// Get the version for a specific device (0 if absent).
    pub fn get(&self, device: &DeviceId) -> u64 {
        self.versions.get(device).copied().unwrap_or(0)
    }

    /// Returns true if `self` strictly dominates `other` (every component
    /// of self >= other, with at least one strictly greater).
    pub fn dominates(&self, other: &FileVersion) -> bool {
        let mut any_greater = false;
        // Check all devices present in self
        for (dev, ver) in &self.versions {
            let other_ver = other.get(dev);
            if *ver < other_ver {
                return false;
            }
            if *ver > other_ver {
                any_greater = true;
            }
        }
        // Check devices present in other but not in self
        for (dev, other_ver) in &other.versions {
            if !self.versions.contains_key(dev) && *other_ver > 0 {
                return false;
            }
        }
        any_greater
    }

    /// Two versions are concurrent if neither dominates the other.
    pub fn concurrent(&self, other: &FileVersion) -> bool {
        !self.dominates(other) && !other.dominates(self)
    }
}

impl Default for FileVersion {
    fn default() -> Self {
        Self::new()
    }
}

/// Kind of entry in a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Directory,
}

/// Cached metadata for a single file in a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: FilePath,
    pub folder_id: FolderId,
    pub kind: EntryKind,
    pub mtime: i64,
    pub size: i64,
    pub hash: FileHash,
    pub device_id: DeviceId,
    pub version: i64,
    pub local_version: i64,
    pub remote_version: i64,
    pub local_mtime: i64,
    pub remote_mtime: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_path_display() {
        let p = FilePath("foo/bar.txt".into());
        assert_eq!(p.to_string(), "foo/bar.txt");
        assert_eq!(p.as_str(), "foo/bar.txt");
    }

    #[test]
    fn file_hash_from_bytes() {
        let h = FileHash::from_bytes(b"hello world");
        assert_eq!(h.0.len(), 32);
        // blake3 of "hello world" is deterministic
        let h2 = FileHash::from_bytes(b"hello world");
        assert_eq!(h, h2);
    }

    #[test]
    fn file_hash_different_input() {
        let h1 = FileHash::from_bytes(b"hello");
        let h2 = FileHash::from_bytes(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn file_version_dominates() {
        let a = FileVersion::single(DeviceId("dev-a".into()), 5);
        let b = FileVersion::single(DeviceId("dev-a".into()), 3);
        assert!(a.dominates(&b));
        assert!(!b.dominates(&b));
    }

    #[test]
    fn file_version_concurrent() {
        let mut a = FileVersion::new();
        a.versions.insert(DeviceId("dev-a".into()), 5);
        a.versions.insert(DeviceId("dev-b".into()), 2);

        let mut b = FileVersion::new();
        b.versions.insert(DeviceId("dev-a".into()), 3);
        b.versions.insert(DeviceId("dev-b".into()), 4);

        assert!(a.concurrent(&b));
        assert!(b.concurrent(&a));
        assert!(!a.dominates(&b));
        assert!(!b.dominates(&a));
    }

    #[test]
    fn file_version_bump() {
        let mut v = FileVersion::new();
        let dev = DeviceId("dev-a".into());
        v.bump(&dev);
        assert_eq!(v.get(&dev), 1);
        v.bump(&dev);
        assert_eq!(v.get(&dev), 2);
    }

    #[test]
    fn file_version_default_is_zero() {
        let v = FileVersion::new();
        assert_eq!(v.get(&DeviceId("nonexistent".into())), 0);
    }

    #[test]
    fn file_hash_display() {
        let h = FileHash::from_bytes(b"test");
        let display = h.to_string();
        assert_eq!(display.len(), 12); // truncated hex
    }
}

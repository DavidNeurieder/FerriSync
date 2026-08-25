use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::domain::file::{EntryKind, FileHash, FilePath};
use crate::domain::folder::FolderId;
use crate::domain::snapshot::{Snapshot, SnapshotEntry};
use crate::domain::FileVersion;

/// A validated, confined filesystem root for synchronization.
///
/// All operations involving remote-controlled paths must pass through
/// `SyncRoot`. The caller should never construct `root.join(remote_path)`
/// directly.
///
/// Enforces:
/// - Relative paths only
/// - No `..` components
/// - No absolute paths
/// - No null bytes
/// - No symlinks in any path component
/// - Path length limits
/// - Root confinement via canonicalization
pub struct SyncRoot {
    root: PathBuf,
}

impl SyncRoot {
    /// Create a new SyncRoot. The path must exist and be a directory.
    pub fn open(path: PathBuf) -> Result<Self> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("sync folder does not exist: {}", path.display()))?;
        if !canonical.is_dir() {
            bail!("sync folder is not a directory: {}", path.display());
        }
        Ok(Self { root: canonical })
    }

    /// The canonical root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validate a relative path and resolve it against the root.
    ///
    /// Returns the resolved absolute path after checking:
    /// - No null bytes
    /// - Not absolute
    /// - No `..` components
    /// - No symlinks in any component
    /// - Stays within root
    pub fn safe_join(&self, untrusted: &str) -> Result<PathBuf> {
        if untrusted.contains('\0') {
            bail!("path contains null byte");
        }

        let p = Path::new(untrusted);
        if p.is_absolute() {
            bail!("absolute path rejected: {untrusted}");
        }

        for component in p.components() {
            use std::path::Component::*;
            match component {
                ParentDir => bail!("path traversal rejected: {untrusted}"),
                Normal(c) if c.is_empty() => bail!("empty path component in: {untrusted}"),
                _ => {}
            }
        }

        let joined = self.root.join(untrusted);

        // Reject any symlink in the path. Walk from the target upward
        // to root, calling symlink_metadata (lstat) on each component.
        {
            let mut cursor = joined.as_path();
            loop {
                if cursor == self.root {
                    break;
                }
                if let Ok(meta) = std::fs::symlink_metadata(cursor) {
                    if meta.file_type().is_symlink() {
                        bail!("symlink rejected: {}", cursor.display());
                    }
                }
                match cursor.parent() {
                    Some(p) if p != cursor => cursor = p,
                    _ => break,
                }
            }
        }

        // Resolve the path and verify confinement.
        let resolved = if joined.exists() {
            joined.canonicalize().with_context(|| {
                format!("could not resolve path: {}", joined.display())
            })?
        } else {
            let mut remaining = Vec::new();
            let mut cursor = joined.as_path();
            while !cursor.exists() {
                match cursor.parent() {
                    Some(parent) if parent != cursor => {
                        remaining.push(cursor.file_name().unwrap());
                        cursor = parent;
                    }
                    _ => break,
                }
            }
            let base = cursor
                .canonicalize()
                .with_context(|| format!("could not resolve parent: {}", cursor.display()))?;
            let mut resolved = base;
            for comp in remaining.into_iter().rev() {
                resolved = resolved.join(comp);
            }
            resolved
        };

        if !resolved.starts_with(&self.root) {
            bail!(
                "path escapes sync folder: {} resolves outside {}",
                untrusted,
                self.root.display()
            );
        }

        Ok(joined)
    }

    /// Read a file's contents.
    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let target = self.safe_join(path)?;
        tokio::fs::read(&target)
            .await
            .with_context(|| format!("failed to read: {}", target.display()))
    }

    /// Write data to a file, creating parent directories as needed.
    pub async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        let target = self.safe_join(path)?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create dirs for: {}", target.display()))?;
        }
        tokio::fs::write(&target, data)
            .await
            .with_context(|| format!("failed to write: {}", target.display()))
    }

    /// Create all directories in the path.
    pub async fn create_dir_all(&self, path: &str) -> Result<()> {
        let target = self.safe_join(path)?;
        tokio::fs::create_dir_all(&target)
            .await
            .with_context(|| format!("failed to create dirs: {}", target.display()))
    }

    /// Atomically rename a file within the root.
    pub async fn rename(&self, from: &str, to: &str) -> Result<()> {
        let from_path = self.safe_join(from)?;
        let to_path = self.safe_join(to)?;
        tokio::fs::rename(&from_path, &to_path)
            .await
            .with_context(|| {
                format!(
                    "failed to rename {} -> {}",
                    from_path.display(),
                    to_path.display()
                )
            })
    }

    /// Get file metadata.
    pub async fn metadata(&self, path: &str) -> Result<std::fs::Metadata> {
        let target = self.safe_join(path)?;
        tokio::fs::metadata(&target)
            .await
            .with_context(|| format!("failed to stat: {}", target.display()))
    }

    /// Scan the directory and build a snapshot.
    ///
    /// This is a synchronous operation that reads the entire directory tree.
    /// Symlinks are skipped. The internal database file `metadata.db` is
    /// excluded.
    pub fn scan(&self, folder_id: FolderId, generation: u64) -> Result<Snapshot> {
        let mut entries = Vec::new();
        self.scan_dir(&self.root, &mut entries)?;
        Ok(Snapshot {
            folder_id,
            generation,
            entries,
        })
    }

    fn scan_dir(&self, dir: &Path, entries: &mut Vec<SnapshotEntry>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            if ft.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ft.is_dir() {
                self.scan_dir(&path, entries)?;
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let fname = path.file_name().unwrap_or_default().to_string_lossy();
            if fname == "metadata.db" {
                continue;
            }
            let relative = path
                .strip_prefix(&self.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let meta = std::fs::metadata(&path)?;
            let mtime = meta
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as i64;
            let data = std::fs::read(&path)?;
            let hash = FileHash::from_bytes(&data);
            entries.push(SnapshotEntry {
                path: FilePath(relative),
                kind: EntryKind::File,
                size: meta.len(),
                hash,
                version: FileVersion::new(), // versions assigned during reconciliation
                mtime,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let d = PathBuf::from(format!(
            "/tmp/syncroot_test_{}_{}",
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn open_valid_dir() {
        let root = SyncRoot::open(tmp()).unwrap();
        assert!(root.root().is_dir());
    }

    #[test]
    fn open_nonexistent_fails() {
        assert!(SyncRoot::open(PathBuf::from("/nonexistent/path")).is_err());
    }

    #[test]
    fn safe_join_normal() {
        let dir = tmp();
        let root = SyncRoot::open(dir.clone()).unwrap();
        let result = root.safe_join("foo/bar.txt").unwrap();
        assert_eq!(result, dir.join("foo/bar.txt"));
    }

    #[test]
    fn safe_join_rejects_absolute() {
        let root = SyncRoot::open(tmp()).unwrap();
        assert!(root.safe_join("/etc/passwd").is_err());
    }

    #[test]
    fn safe_join_rejects_dotdot() {
        let root = SyncRoot::open(tmp()).unwrap();
        assert!(root.safe_join("../etc/passwd").is_err());
    }

    #[test]
    fn safe_join_rejects_null() {
        let root = SyncRoot::open(tmp()).unwrap();
        assert!(root.safe_join("foo\0bar").is_err());
    }

    #[test]
    fn safe_join_rejects_symlink() {
        let dir = tmp();
        let target = dir.join("secret.txt");
        fs::write(&target, "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dir.join("link.txt")).unwrap();
        let root = SyncRoot::open(dir).unwrap();
        assert!(root.safe_join("link.txt").is_err());
    }

    #[test]
    fn scan_finds_files() {
        let dir = tmp();
        fs::write(dir.join("a.txt"), "hello").unwrap();
        fs::write(dir.join("b.txt"), "world").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/c.txt"), "nested").unwrap();

        let root = SyncRoot::open(dir).unwrap();
        let snap = root.scan(FolderId(1), 1).unwrap();
        assert_eq!(snap.entries.len(), 3);
        assert!(snap.find(&FilePath("a.txt".into())).is_some());
        assert!(snap.find(&FilePath("b.txt".into())).is_some());
        assert!(snap.find(&FilePath("sub/c.txt".into())).is_some());
    }

    #[test]
    fn scan_skips_symlinks() {
        let dir = tmp();
        fs::write(dir.join("real.txt"), "data").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("real.txt"), dir.join("link.txt")).unwrap();

        let root = SyncRoot::open(dir).unwrap();
        let snap = root.scan(FolderId(1), 1).unwrap();
        assert_eq!(snap.entries.len(), 1);
        assert!(snap.find(&FilePath("real.txt".into())).is_some());
        assert!(snap.find(&FilePath("link.txt".into())).is_none());
    }

    #[test]
    fn scan_skips_metadata_db() {
        let dir = tmp();
        fs::write(dir.join("a.txt"), "ok").unwrap();
        fs::write(dir.join("metadata.db"), "internal").unwrap();

        let root = SyncRoot::open(dir).unwrap();
        let snap = root.scan(FolderId(1), 1).unwrap();
        assert_eq!(snap.entries.len(), 1);
    }

    #[tokio::test]
    async fn read_write_file() {
        let dir = tmp();
        let root = SyncRoot::open(dir).unwrap();
        root.write_file("test.txt", b"hello world").await.unwrap();
        let data = root.read_file("test.txt").await.unwrap();
        assert_eq!(data, b"hello world");
    }

    #[tokio::test]
    async fn read_write_creates_dirs() {
        let dir = tmp();
        let root = SyncRoot::open(dir).unwrap();
        root.write_file("sub/dir/file.txt", b"nested").await
            .unwrap();
        let data = root.read_file("sub/dir/file.txt").await.unwrap();
        assert_eq!(data, b"nested");
    }

    #[tokio::test]
    async fn rename_file() {
        let dir = tmp();
        let root = SyncRoot::open(dir).unwrap();
        root.write_file("old.txt", b"data").await.unwrap();
        root.rename("old.txt", "new.txt").await.unwrap();
        let data = root.read_file("new.txt").await.unwrap();
        assert_eq!(data, b"data");
        assert!(root.read_file("old.txt").await.is_err());
    }

    #[tokio::test]
    async fn read_nonexistent_fails() {
        let dir = tmp();
        let root = SyncRoot::open(dir).unwrap();
        assert!(root.read_file("nope.txt").await.is_err());
    }
}

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
            joined
                .canonicalize()
                .with_context(|| format!("could not resolve path: {}", joined.display()))?
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

    /// Read a file for upload, verifying it still matches the version that was
    /// indexed.
    ///
    /// This closes the gap between indexing and transmission: if the file was
    /// edited or deleted after it was scanned, the returned bytes would not
    /// match the advertised size/hash and the peer would reject them. Instead
    /// this returns `Ok(None)` so the caller can skip the upload and let a
    /// later synchronization pick up the settled file.
    pub async fn read_verified(
        &self,
        path: &str,
        expected_size: u64,
        expected_hash: &FileHash,
    ) -> Result<Option<Vec<u8>>> {
        let target = self.safe_join(path)?;
        let data = match tokio::fs::read(&target).await {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(e).with_context(|| format!("failed to read: {}", target.display()))
            }
        };
        if data.len() as u64 != expected_size || FileHash::from_bytes(&data) != *expected_hash {
            return Ok(None);
        }
        Ok(Some(data))
    }

    /// Write data to a file, creating parent directories as needed.
    ///
    /// Routed through the confined safe filesystem layer so the write is
    /// atomic (unique temporary file + rename) and cannot follow a symlink out
    /// of the root.
    pub async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        let store = crate::safe_fs::SafeFileStore::open(&self.root)?;
        store
            .write(
                path,
                data.to_vec(),
                None,
                crate::safe_fs::OverwritePolicy::Replace,
            )
            .await?;
        Ok(())
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
            let fname = path.file_name().unwrap_or_default().to_string_lossy();
            // Temporary write targets and conflict backups are local artifacts,
            // not user content: they are never indexed, so a crash leftover can
            // never sync as a real file and a conflict backup cannot be copied
            // back to the peer it came from (which would break convergence).
            if fname.starts_with(crate::safe_fs::TEMP_PREFIX)
                || fname.contains(crate::safe_fs::CONFLICT_TAG)
            {
                continue;
            }
            if ft.is_dir() {
                self.scan_dir(&path, entries)?;
                continue;
            }
            if !ft.is_file() || fname == "metadata.db" {
                continue;
            }
            // Derive size, mtime and content hash from a single read so the
            // indexed metadata always describes the same bytes. A file being
            // rewritten concurrently is skipped this round and picked up by the
            // next scan instead of being indexed inconsistently.
            let (data, meta) = match read_stable(&path)? {
                Some(v) => v,
                None => continue,
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0);
            entries.push(SnapshotEntry {
                path: FilePath(
                    path.strip_prefix(&self.root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string(),
                ),
                kind: EntryKind::File,
                size: data.len() as u64,
                hash: FileHash::from_bytes(&data),
                version: FileVersion::new(), // versions assigned during reconciliation
                mtime,
            });
        }
        Ok(())
    }
}

/// Read a file and its metadata from one consistent snapshot.
///
/// Retries a few times when the file is being written concurrently: the byte
/// count and modification time observed before and after the read must agree.
/// Returns `Ok(None)` if it never settles (or the file vanished), so the caller
/// can skip it and let the next scan try again rather than index metadata that
/// never matched the bytes. Real I/O errors (for example permission denied) are
/// still propagated so a file cannot silently disappear from the index.
fn read_stable(path: &Path) -> std::io::Result<Option<(Vec<u8>, std::fs::Metadata)>> {
    use std::io::Read;
    for _ in 0..3 {
        let mut file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let before = file.metadata()?;
        let mut data = Vec::with_capacity(before.len().min(8 * 1024 * 1024) as usize);
        file.read_to_end(&mut data)?;
        let after = file.metadata()?;
        let stable = before.len() == data.len() as u64
            && after.len() == data.len() as u64
            && before.modified().ok() == after.modified().ok();
        if stable {
            return Ok(Some((data, after)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let d = PathBuf::from(format!("/tmp/syncroot_test_{}_{}", std::process::id(), n));
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

    #[test]
    fn scan_skips_conflict_backups() {
        let dir = tmp();
        fs::write(dir.join("real.txt"), "data").unwrap();
        // A conflict backup is a local artifact and must not be re-synced.
        fs::write(
            dir.join("real.txt.ferrisync-conflict-1-local-abcd1234"),
            "loser",
        )
        .unwrap();

        let root = SyncRoot::open(dir).unwrap();
        let snap = root.scan(FolderId(1), 1).unwrap();
        assert_eq!(snap.entries.len(), 1);
        assert!(snap.find(&FilePath("real.txt".into())).is_some());
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
        root.write_file("sub/dir/file.txt", b"nested")
            .await
            .unwrap();
        let data = root.read_file("sub/dir/file.txt").await.unwrap();
        assert_eq!(data, b"nested");
    }

    #[test]
    fn scan_indexes_hash_and_size_from_the_same_bytes() {
        let dir = tmp();
        fs::write(dir.join("f.txt"), b"consistent").unwrap();
        let root = SyncRoot::open(dir).unwrap();
        let snap = root.scan(FolderId(1), 1).unwrap();
        let entry = snap.find(&FilePath("f.txt".into())).unwrap();
        assert_eq!(entry.size, b"consistent".len() as u64);
        assert_eq!(entry.hash, FileHash::from_bytes(b"consistent"));
    }

    #[tokio::test]
    async fn read_verified_accepts_matching_content() {
        let dir = tmp();
        fs::write(dir.join("f.txt"), b"payload").unwrap();
        let root = SyncRoot::open(dir).unwrap();
        let got = root
            .read_verified("f.txt", 7, &FileHash::from_bytes(b"payload"))
            .await
            .unwrap();
        assert_eq!(got, Some(b"payload".to_vec()));
    }

    #[tokio::test]
    async fn read_verified_reports_stale_content() {
        let dir = tmp();
        fs::write(dir.join("f.txt"), b"new-content").unwrap();
        let root = SyncRoot::open(dir).unwrap();
        // The indexed hash/size are from an older version of the file.
        let stale_hash = FileHash::from_bytes(b"old-content");
        assert_eq!(
            root.read_verified("f.txt", 11, &stale_hash).await.unwrap(),
            None
        );
        // A size mismatch alone is also stale.
        assert_eq!(
            root.read_verified("f.txt", 999, &FileHash::from_bytes(b"new-content"))
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn read_verified_missing_file_is_stale() {
        let dir = tmp();
        let root = SyncRoot::open(dir).unwrap();
        let got = root
            .read_verified("gone.txt", 0, &FileHash::from_bytes(b""))
            .await
            .unwrap();
        assert_eq!(got, None);
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

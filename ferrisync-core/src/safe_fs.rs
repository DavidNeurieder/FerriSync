//! Safe, confined filesystem writes.
//!
//! Every remote-controlled write goes through [`SafeFileStore`], which is the
//! single place that resolves a destination, writes bytes, and replaces the
//! final file. It exists so that no caller has to reason about the ordering of
//! validation, temporary files, and atomic replacement:
//!
//! 1. the relative path is resolved strictly — absolute paths, `..`, null
//!    bytes, and any symlink component are rejected ([`SafeFileStore::resolve`]);
//! 2. a per-destination lock serializes competing writers (see
//!    [`SafeFileStore::write`]); two sessions can never interleave the
//!    backup/replace sequence for the same file;
//! 3. the bytes are written to a *unique*, exclusively-created temporary file in
//!    the destination directory — never a shared fixed name;
//! 4. the destination path is re-validated immediately before replacement, so a
//!    directory swapped for a symlink mid-write cannot redirect the rename
//!    outside the root;
//! 5. the destination is replaced with an atomic rename, after the complete,
//!    flushed file already exists on disk.
//!
//! A failure at any step leaves the previous destination untouched and removes
//! the temporary file. The temporary name is prefixed with [`TEMP_PREFIX`]; the
//! snapshot scanner skips that prefix, so a crash that leaves a partial file
//! behind can never be mistaken for a completed transfer.

use crate::path_safety;
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

/// Prefix of in-progress temporary files. The scanner ignores any entry whose
/// name starts with this, so abandoned temporaries never sync as real files.
pub const TEMP_PREFIX: &str = ".ferrisync-tmp";
/// Marker embedded in every conflict backup file name.
pub const CONFLICT_TAG: &str = ".ferrisync-conflict-";

/// Errors from the safe filesystem layer.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The path was rejected before any filesystem operation.
    #[error("unsafe path {path:?}: {reason}")]
    UnsafePath { path: String, reason: String },
    /// The bytes did not match the agreed hash (defense in depth).
    #[error("hash mismatch for {path:?}")]
    HashMismatch { path: String },
    /// An underlying I/O operation failed.
    #[error("filesystem error for {path:?}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// The blocking worker failed (e.g. panicked).
    #[error("blocking write task failed: {0}")]
    Join(String),
}

/// What to do when the destination already holds different content.
#[derive(Debug, Clone)]
pub enum OverwritePolicy {
    /// Replace the destination, discarding the previous version.
    Replace,
    /// Move a copy of the previous version to a conflict backup before
    /// replacing it. `loser_label` records which side the backup holds.
    BackupExisting { loser_label: String },
}

/// The result of a completed write.
#[derive(Debug, Clone)]
pub struct WriteOutcome {
    /// True when the destination already held identical bytes; nothing was
    /// written and no backup was needed.
    pub unchanged: bool,
    /// Absolute path of the conflict backup, when one was created.
    pub conflict_backup: Option<PathBuf>,
}

/// A confined root directory for safe writes.
#[derive(Debug, Clone)]
pub struct SafeFileStore {
    root: PathBuf,
}

impl SafeFileStore {
    /// Open the store on an existing directory, canonicalizing it once.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = root.as_ref();
        let canonical = path.canonicalize().map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(StoreError::UnsafePath {
                path: path.display().to_string(),
                reason: "not a directory".into(),
            });
        }
        Ok(Self { root: canonical })
    }

    /// The canonical root this store is confined to.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve `relative` inside the root, rejecting escapes and symlinks.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, StoreError> {
        path_safety::safe_join(&self.root, relative).map_err(|e| StoreError::UnsafePath {
            path: relative.to_string(),
            reason: format!("{e:#}"),
        })
    }

    /// Commit `data` to `relative` on a blocking worker.
    ///
    /// Takes ownership of `data` so a large received file is moved into the
    /// worker rather than copied.
    pub async fn write(
        &self,
        relative: &str,
        data: Vec<u8>,
        expected_hash: Option<[u8; 32]>,
        policy: OverwritePolicy,
    ) -> Result<WriteOutcome, StoreError> {
        let store = self.clone();
        let relative = relative.to_string();
        tokio::task::spawn_blocking(move || {
            store.write_blocking(&relative, &data, expected_hash, policy)
        })
        .await
        .map_err(|e| StoreError::Join(e.to_string()))?
    }

    /// Synchronous commit — the single implementation every path shares.
    pub fn write_blocking(
        &self,
        relative: &str,
        data: &[u8],
        expected_hash: Option<[u8; 32]>,
        policy: OverwritePolicy,
    ) -> Result<WriteOutcome, StoreError> {
        // Defense in depth: verify the bytes before they can reach the disk.
        if let Some(expected) = expected_hash {
            if blake3::hash(data).as_bytes() != &expected {
                return Err(StoreError::HashMismatch {
                    path: relative.to_string(),
                });
            }
        }

        let target = self.resolve(relative)?;
        let parent = target.parent().ok_or_else(|| StoreError::UnsafePath {
            path: relative.to_string(),
            reason: "destination has no parent directory".into(),
        })?;

        // Serialize competing writers to this exact destination.
        let _lock = destination_lock(&normalize_key(&self.root, relative));
        let _guard = _lock.lock().unwrap_or_else(|e| e.into_inner());

        // Create parent directories one level at a time, refusing to follow a
        // symlink and refusing to leave the root.
        self.ensure_dir(parent, relative)?;

        // An identical destination needs no work and no backup.
        if let Ok(existing) = std::fs::read(&target) {
            if existing == data {
                return Ok(WriteOutcome {
                    unchanged: true,
                    conflict_backup: None,
                });
            }
        }

        // Write the complete file to a unique temporary file first, so a
        // failure here cannot touch the destination.
        let temp_path = parent.join(temp_file_name());
        write_temp(&temp_path, data)?;

        let result = (|| -> Result<Option<PathBuf>, StoreError> {
            // Re-validate immediately before replacing: an attacker may have
            // swapped a directory for a symlink since we resolved the path.
            let _ = self.resolve(relative)?;

            let backup = match &policy {
                OverwritePolicy::BackupExisting { loser_label } => {
                    if target.exists() {
                        Some(backup_conflict(&target, data, loser_label)?)
                    } else {
                        None
                    }
                }
                OverwritePolicy::Replace => None,
            };

            std::fs::rename(&temp_path, &target).map_err(|source| StoreError::Io {
                path: target.display().to_string(),
                source,
            })?;
            Ok(backup)
        })();

        match result {
            Ok(conflict_backup) => {
                sync_dir(parent);
                Ok(WriteOutcome {
                    unchanged: false,
                    conflict_backup,
                })
            }
            Err(e) => {
                let _ = std::fs::remove_file(&temp_path);
                Err(e)
            }
        }
    }

    /// Create every missing component of `parent` (which must be inside the
    /// root), refusing symlinks and non-directories.
    fn ensure_dir(&self, parent: &Path, relative: &str) -> Result<(), StoreError> {
        let rel = parent
            .strip_prefix(&self.root)
            .map_err(|_| StoreError::UnsafePath {
                path: relative.to_string(),
                reason: "parent directory escapes the sync root".into(),
            })?;

        let mut cur = self.root.clone();
        for comp in rel.components() {
            match comp {
                Component::CurDir => continue,
                Component::Normal(name) => cur.push(name),
                other => {
                    return Err(StoreError::UnsafePath {
                        path: relative.to_string(),
                        reason: format!("unexpected path component {other:?}"),
                    });
                }
            }
            match std::fs::symlink_metadata(&cur) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(StoreError::UnsafePath {
                        path: relative.to_string(),
                        reason: format!("symlink in path: {}", cur.display()),
                    });
                }
                Ok(meta) if !meta.is_dir() => {
                    return Err(StoreError::UnsafePath {
                        path: relative.to_string(),
                        reason: format!("not a directory: {}", cur.display()),
                    });
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&cur).map_err(|source| StoreError::Io {
                        path: cur.display().to_string(),
                        source,
                    })?;
                }
                Err(source) => {
                    return Err(StoreError::Io {
                        path: cur.display().to_string(),
                        source,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Write `data` to a brand-new file at `path`, flush and fsync it, and close
/// the handle before returning so the caller can rename it on any platform.
/// On failure the partial file is removed.
fn write_temp(path: &Path, data: &[u8]) -> Result<(), StoreError> {
    let write = (|| -> Result<(), StoreError> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|source| StoreError::Io {
                path: path.display().to_string(),
                source,
            })?;
        file.write_all(data).map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })?;
        file.flush().map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })?;
        file.sync_all().map_err(|source| StoreError::Io {
            path: path.display().to_string(),
            source,
        })
    })();
    if write.is_err() {
        let _ = std::fs::remove_file(path);
    }
    write
}

/// A fresh, collision-free temporary file name in the destination directory.
fn temp_file_name() -> String {
    format!("{TEMP_PREFIX}-{}", uuid::Uuid::new_v4())
}

/// Copy the current destination aside as a conflict backup.
///
/// A copy (rather than a rename) keeps the destination present until the atomic
/// replacement succeeds, so even a failed rename cannot lose the old version.
fn backup_conflict(
    target: &Path,
    incoming: &[u8],
    loser_label: &str,
) -> Result<PathBuf, StoreError> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let short_hash = &blake3::hash(incoming).to_hex()[..8];
    let backup = PathBuf::from(format!(
        "{}{CONFLICT_TAG}{ts}-{loser_label}-{short_hash}",
        target.display()
    ));
    std::fs::copy(target, &backup).map_err(|source| StoreError::Io {
        path: backup.display().to_string(),
        source,
    })?;
    Ok(backup)
}

/// Best-effort directory fsync so a rename survives a crash. Not all platforms
/// or filesystems support opening a directory; failures are ignored.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = std::fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// A destination key with `.` components removed, so `foo/./bar` and `foo/bar`
/// map to the same lock.
fn normalize_key(root: &Path, relative: &str) -> PathBuf {
    let mut key = root.to_path_buf();
    for comp in Path::new(relative).components() {
        if let Component::Normal(name) = comp {
            key.push(name);
        }
    }
    key
}

/// The process-wide per-destination lock table.
///
/// Entries are weak so the table does not grow without bound: once the last
/// holder of a destination lock drops it, the entry is pruned on the next
/// acquire.
fn lock_table() -> &'static Mutex<HashMap<PathBuf, Weak<Mutex<()>>>> {
    static TABLE: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn destination_lock(key: &Path) -> Arc<Mutex<()>> {
    let mut map = lock_table().lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, weak| weak.strong_count() > 0);
    if let Some(existing) = map.get(key).and_then(Weak::upgrade) {
        return existing;
    }
    let lock = Arc::new(Mutex::new(()));
    map.insert(key.to_path_buf(), Arc::downgrade(&lock));
    lock
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn store() -> (tempfile::TempDir, SafeFileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SafeFileStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn writes_and_creates_parents() {
        let (dir, store) = store();
        let out = store
            .write_blocking("a/b/c.txt", b"hello", None, OverwritePolicy::Replace)
            .unwrap();
        assert!(!out.unchanged);
        assert_eq!(fs::read(dir.path().join("a/b/c.txt")).unwrap(), b"hello");
        assert!(out.conflict_backup.is_none());
    }

    #[test]
    fn identical_content_is_unchanged() {
        let (dir, store) = store();
        fs::write(dir.path().join("x.txt"), b"same").unwrap();
        let before = fs::metadata(dir.path().join("x.txt"))
            .unwrap()
            .modified()
            .unwrap();
        let out = store
            .write_blocking("x.txt", b"same", None, OverwritePolicy::Replace)
            .unwrap();
        assert!(out.unchanged);
        assert_eq!(fs::read(dir.path().join("x.txt")).unwrap(), b"same");
        let after = fs::metadata(dir.path().join("x.txt"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after, "an identical write must not touch the file");
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        let (_dir, store) = store();
        assert!(store.resolve("../escape").is_err());
        assert!(store.resolve("/etc/passwd").is_err());
        assert!(store
            .write_blocking("../escape", b"x", None, OverwritePolicy::Replace)
            .is_err());
    }

    #[test]
    fn hash_mismatch_is_rejected_and_destination_kept() {
        let (dir, store) = store();
        fs::write(dir.path().join("keep.txt"), b"original").unwrap();
        let err = store
            .write_blocking(
                "keep.txt",
                b"new",
                Some([0u8; 32]),
                OverwritePolicy::Replace,
            )
            .unwrap_err();
        assert!(matches!(err, StoreError::HashMismatch { .. }));
        assert_eq!(fs::read(dir.path().join("keep.txt")).unwrap(), b"original");
    }

    #[test]
    fn conflict_backup_preserves_old_version() {
        let (dir, store) = store();
        fs::write(dir.path().join("doc.txt"), b"old version").unwrap();
        let out = store
            .write_blocking(
                "doc.txt",
                b"new version",
                None,
                OverwritePolicy::BackupExisting {
                    loser_label: "local".into(),
                },
            )
            .unwrap();
        let backup = out.conflict_backup.expect("backup created");
        assert_eq!(fs::read(&backup).unwrap(), b"old version");
        assert_eq!(
            fs::read(dir.path().join("doc.txt")).unwrap(),
            b"new version"
        );
        assert!(backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("doc.txt.ferrisync-conflict-"));
    }

    #[test]
    fn temporary_files_are_unique_and_cleaned_up() {
        let (dir, store) = store();
        let a = store
            .write_blocking("f.txt", b"one", None, OverwritePolicy::Replace)
            .unwrap();
        assert!(!a.unchanged);
        let b = store
            .write_blocking("f.txt", b"two", None, OverwritePolicy::Replace)
            .unwrap();
        assert!(!b.unchanged);
        assert_eq!(fs::read(dir.path().join("f.txt")).unwrap(), b"two");
        // No temporary files remain after successful commits.
        let leftovers = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(TEMP_PREFIX))
            .count();
        assert_eq!(leftovers, 0, "no temp files should remain");
    }

    /// A commit that fails after the temp file exists must remove it.
    #[test]
    fn failed_replace_removes_temp() {
        let (dir, store) = store();
        // A destination that is a directory makes the final rename fail.
        fs::create_dir(dir.path().join("taken")).unwrap();
        let err = store
            .write_blocking("taken", b"data", None, OverwritePolicy::Replace)
            .unwrap_err();
        assert!(matches!(err, StoreError::Io { .. }));
        assert!(fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .all(|e| !e.file_name().to_string_lossy().starts_with(TEMP_PREFIX)));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_destination() {
        let (dir, store) = store();
        let secret = dir.path().join("secret.txt");
        fs::write(&secret, b"secret").unwrap();
        std::os::unix::fs::symlink(&secret, dir.path().join("link.txt")).unwrap();
        assert!(store.resolve("link.txt").is_err());
        assert!(store
            .write_blocking("link.txt", b"x", None, OverwritePolicy::Replace)
            .is_err());
        assert_eq!(fs::read(&secret).unwrap(), b"secret");
    }
}

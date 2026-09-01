use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::sync_plan::SyncOperation;
use crate::domain::{FilePath, FileVersion};
use crate::filesystem::SyncRoot;

use super::traits::{FileReceiver, FileSender};

#[cfg(test)]
use super::traits::ReceivedFile;
#[cfg(test)]
use crate::domain::file::FileHash;

/// Executes a sync plan against the local filesystem.
///
/// The transfer manager is the only component that performs file I/O
/// during synchronization. It reads files for uploads and writes files
/// for downloads, using `SyncRoot` for all path operations.
///
/// # Atomic writes
///
/// All file writes are atomic: data goes to a temporary file first,
/// then is renamed into place. This prevents partial writes from
/// corrupting the sync folder if the process crashes mid-transfer.
pub struct TransferManager {
    root: Arc<SyncRoot>,
    temp_dir: PathBuf,
}

impl TransferManager {
    /// Create a new transfer manager.
    ///
    /// `temp_dir` is used for atomic write staging. It should be on the
    /// same filesystem as the sync root for efficient renames.
    pub fn new(root: Arc<SyncRoot>, temp_dir: PathBuf) -> Self {
        Self { root, temp_dir }
    }

    /// Execute all upload operations in a plan.
    ///
    /// Reads each file from the local filesystem and passes it to the
    /// sender. Returns a list of successfully sent file paths.
    pub fn execute_uploads(
        &self,
        operations: &[SyncOperation],
        sender: &dyn FileSender,
    ) -> Result<Vec<FilePath>> {
        let mut sent = Vec::new();

        for op in operations {
            let (path, version) = match op {
                SyncOperation::Upload { path, version, .. } => (path, version),
                _ => continue,
            };

            match self.upload_one(path, version, sender) {
                Ok(()) => sent.push(path.clone()),
                Err(e) => {
                    log::warn!("upload failed for {}: {e}", path.as_str());
                }
            }
        }

        Ok(sent)
    }

    /// Execute all download operations in a plan.
    ///
    /// Receives each file from the remote peer and writes it atomically
    /// to the local filesystem. Returns a list of successfully received file paths.
    pub fn execute_downloads(
        &self,
        operations: &[SyncOperation],
        receiver: &dyn FileReceiver,
    ) -> Result<Vec<FilePath>> {
        let mut received = Vec::new();

        for op in operations {
            let path = match op {
                SyncOperation::Download { path, .. } => path,
                _ => continue,
            };

            match self.download_one(path, receiver) {
                Ok(()) => received.push(path.clone()),
                Err(e) => {
                    log::warn!("download failed for {}: {e}", path.as_str());
                }
            }
        }

        Ok(received)
    }

    /// Read a file from the local filesystem.
    pub fn read_file(&self, path: &FilePath) -> Result<Vec<u8>> {
        let full = self.root.safe_join(path.as_str())?;
        std::fs::read(&full).with_context(|| format!("read failed: {}", path.as_str()))
    }

    fn upload_one(
        &self,
        path: &FilePath,
        version: &FileVersion,
        sender: &dyn FileSender,
    ) -> Result<()> {
        let data = self.read_file(path)?;
        sender.send_file(path, &data, version)
    }

    fn download_one(&self, path: &FilePath, receiver: &dyn FileReceiver) -> Result<()> {
        let received = receiver.receive_file(path)?;
        self.write_atomic(path, &received.data)?;
        Ok(())
    }

    /// Write data atomically: write to temp file, then rename into place.
    fn write_atomic(&self, path: &FilePath, data: &[u8]) -> Result<()> {
        let target = self.root.safe_join(path.as_str())?;

        // Ensure parent directory exists
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create dirs failed: {}", parent.display()))?;
        }

        // Write to temp file
        let temp_name = format!(
            ".ferrisync-tmp-{}-{}",
            path.as_str().replace('/', "_"),
            std::process::id()
        );
        let temp_path = self.temp_dir.join(&temp_name);

        std::fs::write(&temp_path, data)
            .with_context(|| format!("temp write failed: {}", temp_path.display()))?;

        // Atomic rename
        std::fs::rename(&temp_path, &target).with_context(|| {
            let _ = std::fs::remove_file(&temp_path);
            format!(
                "atomic rename failed: {} -> {}",
                temp_path.display(),
                target.display()
            )
        })?;

        Ok(())
    }
}

/// A no-op sender for testing.
#[cfg(test)]
pub struct NullSender;

#[cfg(test)]
impl FileSender for NullSender {
    fn send_file(&self, _path: &FilePath, _data: &[u8], _version: &FileVersion) -> Result<()> {
        Ok(())
    }
}

/// A receiver that returns fixed data for testing.
#[cfg(test)]
pub struct MockReceiver {
    pub data: Vec<u8>,
    pub version: FileVersion,
}

#[cfg(test)]
impl FileReceiver for MockReceiver {
    fn receive_file(&self, _path: &FilePath) -> Result<ReceivedFile> {
        Ok(ReceivedFile {
            data: self.data.clone(),
            version: self.version.clone(),
            hash: FileHash::from_bytes(&self.data),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::device::DeviceId;
    use crate::domain::sync_plan::SyncOperation;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn setup() -> (PathBuf, Arc<SyncRoot>, TransferManager) {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let d = PathBuf::from(format!("/tmp/transfer_test_{}_{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let temp_dir = d.join(".tmp");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let root = Arc::new(SyncRoot::open(d.clone()).unwrap());
        let mgr = TransferManager::new(root.clone(), temp_dir);
        (d, root, mgr)
    }

    #[test]
    fn atomic_write_creates_file() {
        let (_d, _root, mgr) = setup();
        mgr.write_atomic(&FilePath("test.txt".into()), b"hello")
            .unwrap();
        let data = mgr.read_file(&FilePath("test.txt".into())).unwrap();
        assert_eq!(data, b"hello");
    }

    #[test]
    fn atomic_write_creates_parent_dirs() {
        let (_d, _root, mgr) = setup();
        mgr.write_atomic(&FilePath("sub/dir/file.txt".into()), b"nested")
            .unwrap();
        let data = mgr.read_file(&FilePath("sub/dir/file.txt".into())).unwrap();
        assert_eq!(data, b"nested");
    }

    #[test]
    fn execute_uploads_sends_files() {
        let (_d, _root, mgr) = setup();
        mgr.write_atomic(&FilePath("a.txt".into()), b"content_a")
            .unwrap();
        mgr.write_atomic(&FilePath("b.txt".into()), b"content_b")
            .unwrap();

        let ops = vec![
            SyncOperation::Upload {
                path: FilePath("a.txt".into()),
                version: FileVersion::single(DeviceId("dev-1".into()), 1),
                size: 9,
            },
            SyncOperation::Upload {
                path: FilePath("b.txt".into()),
                version: FileVersion::single(DeviceId("dev-1".into()), 1),
                size: 9,
            },
        ];

        let sent = mgr.execute_uploads(&ops, &NullSender).unwrap();
        assert_eq!(sent.len(), 2);
    }

    #[test]
    fn execute_downloads_writes_files() {
        let (d, _root, mgr) = setup();
        let receiver = MockReceiver {
            data: b"received".to_vec(),
            version: FileVersion::single(DeviceId("dev-2".into()), 1),
        };

        let ops = vec![SyncOperation::Download {
            path: FilePath("downloaded.txt".into()),
            version: FileVersion::single(DeviceId("dev-2".into()), 1),
            size: 8,
        }];

        let received = mgr.execute_downloads(&ops, &receiver).unwrap();
        assert_eq!(received.len(), 1);

        let content = std::fs::read(d.join("downloaded.txt")).unwrap();
        assert_eq!(content, b"received");
    }

    #[test]
    fn temp_file_cleaned_up_after_write() {
        let (_d, _root, mgr) = setup();
        mgr.write_atomic(&FilePath("final.txt".into()), b"done")
            .unwrap();

        // No temp files should remain
        let entries: Vec<_> = std::fs::read_dir(&mgr.temp_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".ferrisync-tmp")
            })
            .collect();
        assert!(entries.is_empty(), "temp files left behind: {:?}", entries);
    }
}

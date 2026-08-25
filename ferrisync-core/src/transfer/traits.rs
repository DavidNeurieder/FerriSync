use anyhow::Result;
use crate::domain::{FilePath, FileVersion};

/// Progress callback for a single file transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferProgress {
    pub path: FilePath,
    pub bytes_transferred: u64,
    pub total_size: u64,
}

/// Abstraction for sending file data to a remote peer.
///
/// Implementations handle framing, TLS, and network I/O. The transfer
/// manager only cares about chunk boundaries and progress.
pub trait FileSender: Send + Sync {
    /// Send a file's contents to the remote peer.
    ///
    /// The sender must break data into chunks of at most `chunk_size` bytes.
    /// Returns the total number of bytes sent on success.
    fn send_file(&self, path: &FilePath, data: &[u8], version: &FileVersion) -> Result<()>;
}

/// Abstraction for receiving file data from a remote peer.
///
/// Implementations handle framing, TLS, and network I/O. The transfer
/// manager only cares about chunk boundaries and progress.
pub trait FileReceiver: Send + Sync {
    /// Receive a file from the remote peer.
    ///
    /// Returns the received file data, its version, and its hash.
    fn receive_file(&self, path: &FilePath) -> Result<ReceivedFile>;
}

/// Data received for a single file.
#[derive(Debug, Clone)]
pub struct ReceivedFile {
    pub data: Vec<u8>,
    pub version: FileVersion,
    pub hash: crate::domain::FileHash,
}

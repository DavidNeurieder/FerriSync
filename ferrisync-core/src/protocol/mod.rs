use serde::{Deserialize, Serialize};

pub type Path = String;
pub type Version = u64;
pub type Timestamp = i64;

/// Maximum size in bytes for a single control/index frame.
pub const MAX_CONTROL_FRAME: usize = 1024 * 1024; // 1 MiB

/// Maximum size in bytes for a single file chunk frame.
pub const MAX_CHUNK_FRAME: usize = 128 * 1024; // 128 KiB

/// Maximum allowed file size (total_size field in FileChunk).
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB

/// Maximum length for a file path string.
pub const MAX_PATH_LEN: usize = 1024;

/// Maximum number of paths in a single FileRequest.
pub const MAX_FILE_REQUEST_PATHS: usize = 1000;

/// Top-level message exchanged between sync peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Pairing request with device info
    PairRequest(PairRequest),
    /// Pairing response accepting or rejecting
    PairResponse(PairResponse),
    /// Full or partial file index
    Index(Index),
    /// Request for specific files
    FileRequest(FileRequest),
    /// Chunk of file data
    FileChunk(FileChunk),
    /// Acknowledgement of receipt
    Ack(Ack),
    /// Error message
    Error(ErrorMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairRequest {
    pub device_id: String,
    pub device_name: String,
    pub cert_fingerprint: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairResponse {
    pub accepted: bool,
    pub device_id: String,
    pub device_name: String,
    pub cert_fingerprint: Vec<u8>,
    pub reason: Option<String>,
}

/// Entry for a single file in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub path: Path,
    pub local_version: Version,
    pub remote_version: Version,
    pub mtime: Timestamp,
    pub size: u64,
    pub hash: Vec<u8>,
}

/// File index exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub folder_id: String,
    /// The device that produced this index.
    pub device_id: String,
    pub entries: Vec<IndexEntry>,
}

/// Request specific files from the remote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRequest {
    pub folder_id: String,
    pub paths: Vec<Path>,
}

/// A chunk of file data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunk {
    pub folder_id: String,
    pub path: Path,
    pub offset: u64,
    pub data: Vec<u8>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ack {
    pub path: Path,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCode {
    ProtocolVersionMismatch,
    AuthenticationFailed,
    FileNotFound,
    PermissionDenied,
    InternalError,
}

/// Frame a message with length prefix for sending over a stream.
pub fn frame_message(msg: &SyncMessage) -> anyhow::Result<Vec<u8>> {
    let payload = bincode::serialize(msg)?;
    let len = payload.len() as u32;
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(&payload);
    Ok(framed)
}

/// Parse a framed message from raw bytes.
pub fn parse_frame(data: &[u8]) -> anyhow::Result<(SyncMessage, usize)> {
    if data.len() < 4 {
        anyhow::bail!("frame too short");
    }
    let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if len > MAX_CONTROL_FRAME {
        anyhow::bail!("frame too large: {len} bytes exceeds {MAX_CONTROL_FRAME} limit");
    }
    if data.len() < 4 + len {
        anyhow::bail!("incomplete frame");
    }
    let msg: SyncMessage = bincode::deserialize(&data[4..4 + len])?;
    Ok((msg, 4 + len))
}

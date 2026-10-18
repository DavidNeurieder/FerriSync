//! Strict handling of an inbound file-transfer stream.
//!
//! Both the initiating (`run_sync_session`) and responding
//! (`handle_server_session`) sides receive files as a stream of [`FileChunk`]
//! frames. This module centralizes the rules that decide whether that stream is
//! trustworthy:
//!
//! * only files the local plan expects may arrive ([`SessionError::UnexpectedFile`]);
//! * chunks for one file must be contiguous and non-overlapping;
//! * the declared size and the received byte count must match the index;
//! * the final BLAKE3 hash must match the index hash.
//!
//! Reassembly and verification are deliberately separated from filesystem
//! writes so the caller remains responsible for committing verified bytes.

use crate::protocol::{FileChunk, Index, MAX_FILE_SIZE, MAX_PATH_LEN};
use crate::sync::orchestrator::validate_chunk;
use std::collections::HashMap;
use std::path::{Component, Path};
use std::time::Duration;

/// Categories of session failure.
///
/// These are typed so retry/fallback decisions never depend on message text
/// (see `api::classify_peer_error`). A session that does not reach
/// [`CompletedFile`] for every expected file returns one of these rather than a
/// success.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The peer closed the connection before the session completed.
    #[error("peer disconnected before the session completed")]
    PeerDisconnected,
    /// The session made no progress within the idle window.
    #[error("session did not complete within {0:?}")]
    TransferTimeout(Duration),
    /// The peer sent a file that was not part of the agreed transfer set.
    #[error("peer sent unexpected file {0:?}")]
    UnexpectedFile(String),
    /// Chunks did not form a clean, contiguous stream for a file.
    #[error("invalid chunk sequence for {path:?}: {detail}")]
    InvalidChunkSequence { path: String, detail: String },
    /// Declared/received size disagreed with the index.
    #[error("size mismatch for {path:?}: declared {declared} bytes, expected {expected}")]
    SizeMismatch {
        path: String,
        declared: u64,
        expected: u64,
    },
    /// The received bytes did not match the expected BLAKE3 hash.
    #[error("hash mismatch for {path:?}")]
    HashMismatch { path: String },
    /// The remote index was malformed or unsafe.
    #[error("invalid remote index: {0}")]
    InvalidIndex(String),
    /// The session ended before every expected operation was confirmed.
    #[error("session incomplete: {0}")]
    Incomplete(String),
    /// Committing a verified file to disk failed.
    #[error("filesystem error while committing {path:?}: {detail}")]
    Filesystem { path: String, detail: String },
}

/// A fully received and verified file, ready to be committed to disk.
#[derive(Debug)]
pub struct CompletedFile {
    pub path: String,
    pub data: Vec<u8>,
}

/// One file the session expects to receive: its exact size and BLAKE3 hash.
#[derive(Debug, Clone)]
pub struct ExpectedFile {
    pub size: u64,
    pub hash: [u8; 32],
}

#[derive(Debug)]
struct PartialFile {
    total_size: u64,
    next_offset: u64,
    data: Vec<u8>,
}

/// Tracks the files a session expects to receive and reassembles their chunks.
///
/// Every chunk must belong to an expected file; the first chunk of a file must
/// start at offset 0 and each following chunk must continue exactly where the
/// previous one ended. A file is only surfaced once its byte count and BLAKE3
/// hash have been verified against the index.
pub struct IncomingTransfers {
    expected: HashMap<String, ExpectedFile>,
    partials: HashMap<String, PartialFile>,
    buffered_bytes: usize,
    completed: usize,
}

impl IncomingTransfers {
    pub fn new(expected: HashMap<String, ExpectedFile>) -> Self {
        Self {
            expected,
            partials: HashMap::new(),
            buffered_bytes: 0,
            completed: 0,
        }
    }

    /// Number of files the session expects to receive.
    pub fn expected_count(&self) -> usize {
        self.expected.len()
    }

    /// Number of files fully received and verified so far.
    pub fn completed_count(&self) -> usize {
        self.completed
    }

    /// Whether a file is currently mid-transfer (unfinished).
    pub fn has_pending(&self) -> bool {
        !self.partials.is_empty()
    }

    /// Feed one inbound chunk.
    ///
    /// Returns `Ok(Some(file))` exactly once per expected file — on the chunk
    /// that completes it and only after size and hash verification. Returns
    /// `Err` for any chunk that is unexpected, mis-sized, out of order, or has a
    /// bad hash.
    pub fn accept(&mut self, chunk: &FileChunk) -> Result<Option<CompletedFile>, SessionError> {
        let expected = self
            .expected
            .get(&chunk.path)
            .ok_or_else(|| SessionError::UnexpectedFile(chunk.path.clone()))?;

        validate_chunk(chunk, self.partials.len(), self.buffered_bytes).map_err(|e| {
            SessionError::InvalidChunkSequence {
                path: chunk.path.clone(),
                detail: e.to_string(),
            }
        })?;

        if chunk.total_size != expected.size {
            return Err(SessionError::SizeMismatch {
                path: chunk.path.clone(),
                declared: chunk.total_size,
                expected: expected.size,
            });
        }

        if !self.partials.contains_key(&chunk.path) {
            if chunk.offset != 0 {
                return Err(SessionError::InvalidChunkSequence {
                    path: chunk.path.clone(),
                    detail: format!("first chunk offset is {}, expected 0", chunk.offset),
                });
            }
            // Do not pre-allocate from the declared size: a hostile index could
            // claim a huge size to trigger an allocation before any data is
            // verified.
            self.partials.insert(
                chunk.path.clone(),
                PartialFile {
                    total_size: chunk.total_size,
                    next_offset: 0,
                    data: Vec::new(),
                },
            );
        }

        let partial = self.partials.get_mut(&chunk.path).expect("just inserted");
        if partial.total_size != chunk.total_size {
            return Err(SessionError::InvalidChunkSequence {
                path: chunk.path.clone(),
                detail: format!(
                    "total_size changed mid-file from {} to {}",
                    partial.total_size, chunk.total_size
                ),
            });
        }
        if chunk.offset != partial.next_offset {
            return Err(SessionError::InvalidChunkSequence {
                path: chunk.path.clone(),
                detail: format!(
                    "expected offset {}, got {}",
                    partial.next_offset, chunk.offset
                ),
            });
        }
        partial.data.extend_from_slice(&chunk.data);
        partial.next_offset += chunk.data.len() as u64;
        self.buffered_bytes += chunk.data.len();

        if partial.next_offset < partial.total_size {
            return Ok(None);
        }

        // `validate_chunk` guarantees `next_offset <= total_size`, so reaching
        // here means the file is exactly complete.
        let partial = self.partials.remove(&chunk.path).expect("present");
        self.buffered_bytes -= partial.data.len();

        if partial.data.len() as u64 != expected.size {
            return Err(SessionError::SizeMismatch {
                path: chunk.path.clone(),
                declared: partial.data.len() as u64,
                expected: expected.size,
            });
        }
        let actual = blake3::hash(&partial.data);
        if actual.as_bytes() != &expected.hash {
            return Err(SessionError::HashMismatch {
                path: chunk.path.clone(),
            });
        }

        self.completed += 1;
        Ok(Some(CompletedFile {
            path: chunk.path.clone(),
            data: partial.data,
        }))
    }
}

/// Validate every entry of a remote index before it is used to plan transfers.
///
/// Rejects absolute paths, parent/self traversal, duplicate normalized
/// destinations, NUL bytes, over-long paths, and entries that declare more
/// bytes than the protocol allows.
pub fn validate_remote_index(index: &Index) -> Result<(), SessionError> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for entry in &index.entries {
        validate_relative_path(&entry.path)?;
        if !seen.insert(entry.path.as_str()) {
            return Err(SessionError::InvalidIndex(format!(
                "duplicate path {:?}",
                entry.path
            )));
        }
        if entry.size > MAX_FILE_SIZE {
            return Err(SessionError::InvalidIndex(format!(
                "entry {:?} declares {} bytes (max {MAX_FILE_SIZE})",
                entry.path, entry.size
            )));
        }
    }
    Ok(())
}

/// Build the expected-incoming set, requiring a full 32-byte BLAKE3 hash for
/// every entry. A short hash is rejected rather than silently padded.
pub fn expected_files(
    entries: impl IntoIterator<Item = (String, u64, Vec<u8>)>,
) -> Result<HashMap<String, ExpectedFile>, SessionError> {
    let mut expected: HashMap<String, ExpectedFile> = HashMap::new();
    for (path, size, hash) in entries {
        let hash: [u8; 32] = hash.as_slice().try_into().map_err(|_| {
            SessionError::InvalidIndex(format!(
                "entry {path:?} has a {}-byte hash (expected 32)",
                hash.len()
            ))
        })?;
        if expected
            .insert(path.clone(), ExpectedFile { size, hash })
            .is_some()
        {
            return Err(SessionError::InvalidIndex(format!(
                "duplicate expected path {path:?}"
            )));
        }
    }
    Ok(expected)
}

fn validate_relative_path(path: &str) -> Result<(), SessionError> {
    if path.is_empty() {
        return Err(SessionError::InvalidIndex("empty path".into()));
    }
    if path.len() > MAX_PATH_LEN {
        return Err(SessionError::InvalidIndex(format!(
            "path too long ({} bytes, max {MAX_PATH_LEN})",
            path.len()
        )));
    }
    if path.contains('\0') {
        return Err(SessionError::InvalidIndex(format!(
            "path contains NUL byte: {path:?}"
        )));
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(SessionError::InvalidIndex(format!(
            "absolute path rejected: {path:?}"
        )));
    }
    for component in p.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {
                return Err(SessionError::InvalidIndex(format!(
                    "non-normalized path component in {path:?}"
                )));
            }
            _ => {
                return Err(SessionError::InvalidIndex(format!(
                    "path traversal rejected: {path:?}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected(path: &str, data: &[u8]) -> (String, u64, Vec<u8>) {
        (
            path.to_string(),
            data.len() as u64,
            blake3::hash(data).as_bytes().to_vec(),
        )
    }

    fn chunk(path: &str, offset: u64, data: &[u8], total: u64) -> FileChunk {
        FileChunk {
            folder_id: "1".into(),
            path: path.to_string(),
            offset,
            data: data.to_vec(),
            total_size: total,
        }
    }

    fn transfers(entries: Vec<(String, u64, Vec<u8>)>) -> IncomingTransfers {
        IncomingTransfers::new(expected_files(entries).unwrap())
    }

    #[test]
    fn reassembles_and_verifies_a_single_chunk_file() {
        let mut t = transfers(vec![expected("a.txt", b"hello")]);
        let done = t.accept(&chunk("a.txt", 0, b"hello", 5)).unwrap().unwrap();
        assert_eq!(done.path, "a.txt");
        assert_eq!(done.data, b"hello");
        assert_eq!(t.completed_count(), 1);
        assert!(!t.has_pending());
    }

    #[test]
    fn reassembles_multi_chunk_file_in_order() {
        let data = vec![7u8; 10];
        let mut t = transfers(vec![expected("a.bin", &data)]);
        assert!(t
            .accept(&chunk("a.bin", 0, &data[0..4], 10))
            .unwrap()
            .is_none());
        assert!(t
            .accept(&chunk("a.bin", 4, &data[4..7], 10))
            .unwrap()
            .is_none());
        let done = t
            .accept(&chunk("a.bin", 7, &data[7..10], 10))
            .unwrap()
            .unwrap();
        assert_eq!(done.data, data);
    }

    #[test]
    fn rejects_unexpected_file() {
        let mut t = transfers(vec![expected("a.txt", b"hi")]);
        let err = t.accept(&chunk("b.txt", 0, b"hi", 2)).unwrap_err();
        assert!(matches!(err, SessionError::UnexpectedFile(_)));
    }

    #[test]
    fn rejects_hash_mismatch() {
        let mut t = transfers(vec![expected("a.txt", b"hello")]);
        let err = t.accept(&chunk("a.txt", 0, b"HELLO", 5)).unwrap_err();
        assert!(matches!(err, SessionError::HashMismatch { .. }));
    }

    #[test]
    fn rejects_declared_size_mismatch() {
        let mut t = transfers(vec![expected("a.txt", b"hello")]);
        let err = t.accept(&chunk("a.txt", 0, b"hello", 99)).unwrap_err();
        assert!(matches!(err, SessionError::SizeMismatch { .. }));
    }

    #[test]
    fn rejects_out_of_order_chunks() {
        let data = vec![0u8; 8];
        let mut t = transfers(vec![expected("a.bin", &data)]);
        t.accept(&chunk("a.bin", 0, &data[0..4], 8)).unwrap();
        let err = t.accept(&chunk("a.bin", 6, &data[6..8], 8)).unwrap_err();
        assert!(matches!(err, SessionError::InvalidChunkSequence { .. }));
    }

    #[test]
    fn rejects_overlapping_chunks() {
        let data = vec![0u8; 8];
        let mut t = transfers(vec![expected("a.bin", &data)]);
        t.accept(&chunk("a.bin", 0, &data[0..4], 8)).unwrap();
        let err = t.accept(&chunk("a.bin", 2, &data[2..6], 8)).unwrap_err();
        assert!(matches!(err, SessionError::InvalidChunkSequence { .. }));
    }

    #[test]
    fn rejects_first_chunk_not_at_zero() {
        let data = vec![0u8; 8];
        let mut t = transfers(vec![expected("a.bin", &data)]);
        let err = t.accept(&chunk("a.bin", 4, &data[4..8], 8)).unwrap_err();
        assert!(matches!(err, SessionError::InvalidChunkSequence { .. }));
    }

    #[test]
    fn empty_file_is_a_single_empty_chunk() {
        let mut t = transfers(vec![expected("empty.txt", b"")]);
        let done = t.accept(&chunk("empty.txt", 0, b"", 0)).unwrap().unwrap();
        assert!(done.data.is_empty());
    }

    #[test]
    fn validate_remote_index_rejects_bad_entries() {
        let index = |paths: &[&str]| Index {
            folder_id: "1".into(),
            device_id: "d".into(),
            remote_path: None,
            entries: paths
                .iter()
                .map(|p| crate::protocol::IndexEntry {
                    path: p.to_string(),
                    local_version: 1,
                    remote_version: 0,
                    mtime: 0,
                    size: 1,
                    hash: vec![0u8; 32],
                })
                .collect(),
        };
        assert!(validate_remote_index(&index(&["a/b.txt"])).is_ok());
        assert!(validate_remote_index(&index(&["/etc/passwd"])).is_err());
        assert!(validate_remote_index(&index(&["../escape"])).is_err());
        assert!(validate_remote_index(&index(&["a/../../b"])).is_err());
        assert!(validate_remote_index(&index(&["a.txt", "a.txt"])).is_err());
    }

    #[test]
    fn expected_files_requires_full_hash() {
        assert!(expected_files(vec![("a".into(), 1, vec![0u8; 16])]).is_err());
        assert!(expected_files(vec![("a".into(), 1, vec![0u8; 32])]).is_ok());
    }
}

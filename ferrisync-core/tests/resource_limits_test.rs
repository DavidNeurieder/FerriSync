//! Milestone 5 — resource-limit enforcement.
//!
//! These pin the hard bounds that keep a hostile or buggy peer from exhausting
//! the local process: index entry count and declared sizes, chunk size, path
//! length, per-session in-flight files, buffered bytes, and the
//! concurrent-connection cap. The pure validators are exercised directly so the
//! exact thresholds are locked in without needing a live socket; the connection
//! cap is checked end-to-end against the real accept loop.

use ferrisync_core::protocol::{
    FileChunk, Index, IndexEntry, MAX_CHUNK_FRAME, MAX_FILE_SIZE, MAX_INDEX_ENTRIES, MAX_PATH_LEN,
};
use ferrisync_core::sync::orchestrator::validate_chunk;
use ferrisync_core::sync_engine::session_transfer::validate_remote_index;

fn entry(path: &str, size: u64) -> IndexEntry {
    IndexEntry {
        path: path.to_string(),
        local_version: 1,
        remote_version: 0,
        mtime: 1,
        size,
        hash: vec![0u8; 32],
    }
}

fn index_with(entries: Vec<IndexEntry>) -> Index {
    Index {
        folder_id: "1".into(),
        device_id: "remote".into(),
        remote_path: None,
        entries,
    }
}

fn chunk(path: &str, offset: u64, len: usize, total_size: u64) -> FileChunk {
    FileChunk {
        folder_id: "1".into(),
        path: path.to_string(),
        offset,
        data: vec![0u8; len],
        total_size,
    }
}

#[test]
fn index_with_more_than_max_entries_is_rejected() {
    let entries = (0..MAX_INDEX_ENTRIES + 1)
        .map(|i| entry(&format!("f{i}"), 1))
        .collect();
    let err = validate_remote_index(&index_with(entries)).unwrap_err();
    assert!(
        format!("{err}").contains("entries"),
        "expected an entry-count rejection, got: {err}"
    );
}

#[test]
fn index_at_the_entry_limit_is_accepted() {
    let entries = (0..MAX_INDEX_ENTRIES)
        .map(|i| entry(&format!("f{i}"), 1))
        .collect();
    assert!(validate_remote_index(&index_with(entries)).is_ok());
}

#[test]
fn index_entry_declaring_more_than_max_file_size_is_rejected() {
    let idx = index_with(vec![entry("too-big", MAX_FILE_SIZE + 1)]);
    assert!(validate_remote_index(&idx).is_err());
}

#[test]
fn chunk_with_overlong_path_is_rejected() {
    let path = "a".repeat(MAX_PATH_LEN + 1);
    assert!(validate_chunk(&chunk(&path, 0, 1, 1), 0, 0).is_err());
}

#[test]
fn chunk_larger_than_the_frame_cap_is_rejected() {
    let oversized = chunk("f", 0, MAX_CHUNK_FRAME + 1, (MAX_CHUNK_FRAME + 1) as u64);
    assert!(validate_chunk(&oversized, 0, 0).is_err());
}

#[test]
fn chunk_declaring_more_than_max_file_size_is_rejected() {
    assert!(validate_chunk(&chunk("f", 0, 1, MAX_FILE_SIZE + 1), 0, 0).is_err());
}

#[test]
fn chunk_extending_past_declared_total_is_rejected() {
    // offset 4 + len 4 = 8 > total_size 6.
    assert!(validate_chunk(&chunk("f", 4, 4, 6), 0, 0).is_err());
}

#[test]
fn too_many_in_flight_files_are_rejected() {
    assert!(
        validate_chunk(&chunk("f", 0, 1, 1), 64, 0).is_err(),
        "a 65th in-flight file must be rejected"
    );
    assert!(
        validate_chunk(&chunk("f", 0, 1, 1), 63, 0).is_ok(),
        "the 64th in-flight file must still be allowed"
    );
}

#[test]
fn buffered_byte_limit_is_enforced() {
    const CAP: usize = 256 * 1024 * 1024;
    // 16 more bytes would cross the cap.
    assert!(validate_chunk(&chunk("f", 0, 16, 16), 0, CAP - 8).is_err());
    // Exactly filling the cap is fine.
    assert!(validate_chunk(&chunk("f", 0, 16, 16), 0, CAP - 16).is_ok());
}

#[test]
fn declared_size_is_never_preallocated() {
    // A hostile index can declare a 10 GiB file, but the receiver must not
    // allocate from that declaration: the first (tiny) chunk is buffered and the
    // file simply stays incomplete. A `Vec::with_capacity(total_size)` here
    // would abort the process, so this also guards against that regression.
    use ferrisync_core::sync_engine::session_transfer::{ExpectedFile, IncomingTransfers};
    use std::collections::HashMap;

    let mut expected = HashMap::new();
    expected.insert(
        "huge".to_string(),
        ExpectedFile {
            size: MAX_FILE_SIZE,
            hash: [0u8; 32],
        },
    );
    let mut transfers = IncomingTransfers::new(expected);
    let first = chunk("huge", 0, 1, MAX_FILE_SIZE);
    assert!(matches!(transfers.accept(&first), Ok(None)));
    assert!(transfers.has_pending());
}

#[tokio::test]
async fn concurrent_connection_cap_sheds_excess_connections() {
    use ferrisync_core::crypto::{cert_to_device_id, CryptoProvider};
    use ferrisync_core::persistence::InMemoryStateStore;
    use ferrisync_core::storage::Storage;
    use ferrisync_core::sync_engine::server::{serve_folder, PairPolicy};
    use ferrisync_core::sync_engine::session::MAX_SESSIONS;
    use ferrisync_core::DeviceInfo;
    use std::sync::Arc;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpStream;

    let dir = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let crypto = Arc::new(CryptoProvider::generate().unwrap());
    let id = cert_to_device_id(crypto.certificate().await.as_ref()).to_string();
    let storage = Arc::new(Storage::open(&data.path().join("metadata.db")).unwrap());
    storage.upsert_device(&id, "owner", None, None).unwrap();
    storage
        .add_sync_folder(dir.path().to_str().unwrap(), &id, "bidirectional")
        .unwrap();
    let info = DeviceInfo {
        id,
        name: "owner".into(),
        cert_fingerprint: crypto.fingerprint().await,
    };
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    };
    let (server, _events) = serve_folder(
        storage,
        crypto,
        info,
        dir.path().to_str().unwrap().to_string(),
        port,
        PairPolicy::AutoAccept,
        Arc::new(InMemoryStateStore::new()),
    )
    .await
    .unwrap();
    let addr = format!("127.0.0.1:{port}");

    // Occupy every session slot with connections that never finish the TLS
    // handshake; each holds its permit until the handshake timeout expires.
    let mut held = Vec::new();
    for _ in 0..MAX_SESSIONS {
        held.push(TcpStream::connect(&addr).await.unwrap());
    }
    // Let the accept loop hand every slot out.
    tokio::time::sleep(std::time::Duration::from_millis(750)).await;

    // The next connection must be shed immediately (closed), not served.
    let mut extra = TcpStream::connect(&addr).await.unwrap();
    let mut buf = [0u8; 1];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), extra.read(&mut buf))
        .await
        .expect("excess connection was not closed by the server")
        .unwrap();
    assert_eq!(n, 0, "server must close connections beyond MAX_SESSIONS");

    drop(held);
    server.stop().await;
}

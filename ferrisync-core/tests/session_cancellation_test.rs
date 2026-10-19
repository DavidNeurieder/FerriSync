//! Milestone 5 acceptance tests: cancellation and cleanup.
//!
//! Cancelling a sync drops the session future — the same thing that happens
//! when a user cancels a sync, the app shuts down, or the caller simply stops
//! polling. These tests cancel an in-flight session by dropping its future and
//! assert the Milestone 5 guarantee that cancellation releases tasks, locks,
//! and temporary files:
//!
//! * a cancelled download commits no destination and strands no temp file;
//! * a cancelled upload leaves the source and root untouched;
//! * a cancelled session is never recorded as a *completed* one;
//! * the very next session resumes and succeeds.

mod support;

use ferrisync_core::protocol::{FileChunk, SyncMessage};
use ferrisync_core::safe_fs::TEMP_PREFIX;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use support::*;

/// The payload the scripted peer advertises. Kept tiny so the partial delivery
/// stays well inside the in-flight buffer cap.
const PAYLOAD: &[u8] = b"0123456789abcdef";

/// How long to let a session block on a silent peer before cancelling it. Far
/// longer than a loopback handshake, far shorter than the session idle timeout.
const CANCEL_AFTER: Duration = Duration::from_millis(1500);

fn stranded_temp_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(TEMP_PREFIX))
        .collect();
    names.sort();
    names
}

fn completed_sessions(a: &TestSide) -> usize {
    a.storage
        .list_recent_sessions(16)
        .unwrap()
        .iter()
        .filter(|s| s.status == "complete")
        .count()
}

fn hash_of(data: &[u8]) -> Vec<u8> {
    blake3::hash(data).as_bytes().to_vec()
}

fn fresh_crypto() -> Arc<ferrisync_core::crypto::CryptoProvider> {
    Arc::new(ferrisync_core::crypto::CryptoProvider::generate().unwrap())
}

/// A peer that announces `path` with `size`/`hash`, delivers only `partial`
/// bytes of it, and then holds the connection open without ever finishing.
///
/// `crypto` is the identity the peer presents; callers that later reconnect to
/// a real server for the same device must pass that server's identity so the
/// initiator's trust-on-first-use binding still matches.
async fn spawn_stalling_download(
    crypto: Arc<ferrisync_core::crypto::CryptoProvider>,
    device_id: &str,
    path: &str,
    size: u64,
    hash: Vec<u8>,
    partial: Vec<u8>,
) -> std::net::SocketAddr {
    let mut index = empty_index(device_id);
    index.entries.push(index_entry(path, size, &hash));
    let path = path.to_string();
    spawn_scripted_peer(crypto, index, move |mut stream| async move {
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path,
                offset: 0,
                data: partial,
                total_size: size,
            }),
        )
        .await;
        // Stay connected but silent: the initiator blocks waiting for the rest,
        // which is exactly the state we want to cancel from.
        tokio::time::sleep(Duration::from_secs(3600)).await;
    })
    .await
}

/// A peer with an empty index that completes the handshake and then goes silent,
/// so any outgoing upload is left unacknowledged.
async fn spawn_silent_peer(device_id: &str) -> std::net::SocketAddr {
    let crypto = Arc::new(ferrisync_core::crypto::CryptoProvider::generate().unwrap());
    spawn_scripted_peer(crypto, empty_index(device_id), |stream| async move {
        // Hold the connection open (dropping it would surface as a broken pipe)
        // but never acknowledge the upload.
        let _held = stream;
        tokio::time::sleep(Duration::from_secs(3600)).await;
    })
    .await
}

// ────────────────────────────────────────────────────────────────────
// A cancelled download leaves no destination and no temp file.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cancelled_download_leaves_no_destination_and_no_temp_file() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let addr = spawn_stalling_download(
        fresh_crypto(),
        &id_b,
        "cancel.bin",
        PAYLOAD.len() as u64,
        hash_of(PAYLOAD),
        PAYLOAD[..PAYLOAD.len() / 2].to_vec(),
    )
    .await;

    let cancelled = tokio::time::timeout(CANCEL_AFTER, run_client(&a, addr, &id_b)).await;
    assert!(
        cancelled.is_err(),
        "the session must still be blocked on the peer when the test cancels it"
    );

    assert!(
        !a.dir.path().join("cancel.bin").exists(),
        "a cancelled download must never commit a destination file"
    );
    assert!(
        stranded_temp_files(a.dir.path()).is_empty(),
        "a cancelled download must not strand a temporary file: {:?}",
        stranded_temp_files(a.dir.path())
    );
    assert_eq!(
        completed_sessions(&a),
        0,
        "a cancelled session must never be recorded as complete"
    );
}

// ────────────────────────────────────────────────────────────────────
// A cancelled upload leaves the source file and the root untouched.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cancelled_upload_leaves_source_and_root_clean() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    a.write("up.bin", PAYLOAD);

    let addr = spawn_silent_peer(&id_b).await;

    let cancelled = tokio::time::timeout(CANCEL_AFTER, run_client(&a, addr, &id_b)).await;
    assert!(
        cancelled.is_err(),
        "the upload must still be unacknowledged when the test cancels it: {cancelled:?}"
    );

    assert_eq!(
        a.read("up.bin"),
        PAYLOAD,
        "cancelling an upload must not disturb the source file"
    );
    assert!(
        stranded_temp_files(a.dir.path()).is_empty(),
        "a cancelled upload must not strand a temporary file: {:?}",
        stranded_temp_files(a.dir.path())
    );
    assert_eq!(
        completed_sessions(&a),
        0,
        "a cancelled session must never be recorded as complete"
    );
}

// ────────────────────────────────────────────────────────────────────
// A cancelled session leaves the peer usable: the next session succeeds.
// ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn session_can_be_retried_after_a_cancellation() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");
    b.write("recover.bin", PAYLOAD);

    // First attempt: cancelled mid-download against a peer that stalls. It uses
    // the same identity as the real server below, so the initiator's
    // trust-on-first-use binding from this attempt still matches on retry.
    let stall_addr = spawn_stalling_download(
        b.crypto.clone(),
        &id_b,
        "recover.bin",
        PAYLOAD.len() as u64,
        hash_of(PAYLOAD),
        PAYLOAD[..1].to_vec(),
    )
    .await;
    let cancelled = tokio::time::timeout(CANCEL_AFTER, run_client(&a, stall_addr, &id_b)).await;
    assert!(
        cancelled.is_err(),
        "the first attempt must be cancelled while still in flight"
    );
    assert_eq!(
        completed_sessions(&a),
        0,
        "the cancelled attempt must not be recorded as complete"
    );

    // Retry against the real server: the cancelled session must not have left
    // the client's storage or root in a state that blocks recovery.
    let addr = spawn_real_server(&b, &id_b).await;
    let result = run_client(&a, addr, &id_b)
        .await
        .expect("a session must be retryable after a cancellation");
    assert!(
        result.pulled.contains(&"recover.bin".to_string()),
        "the retry must pull the file, got {:?}",
        result.pulled
    );
    assert_eq!(
        a.read("recover.bin"),
        PAYLOAD,
        "the retry must deliver the complete file"
    );
    assert!(
        stranded_temp_files(a.dir.path()).is_empty(),
        "the retry must not strand a temporary file: {:?}",
        stranded_temp_files(a.dir.path())
    );
    assert_eq!(
        completed_sessions(&a),
        1,
        "the successful retry must be recorded exactly once"
    );
}

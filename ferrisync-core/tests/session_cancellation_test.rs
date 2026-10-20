//! Milestone 5 acceptance tests: cancellation and cleanup.
//!
//! Cancelling a sync drops the session future — the same thing that happens
//! when a user cancels a sync, the app shuts down, or the caller simply stops
//! polling. These tests cancel an in-flight session by dropping its future and
//! assert the Milestone 5 guarantee that cancellation releases tasks, locks,
//! and temporary files:
//!
//! * a cancelled download never commits a partial destination;
//! * an existing destination is preserved across a cancellation;
//! * a cancelled upload leaves the source and root untouched;
//! * a cancelled session is never recorded as a *completed* one;
//! * the very next session resumes and succeeds;
//! * repeated cancellation does not accumulate local state.
//!
//! ## Determinism
//!
//! No test here waits for a fixed wall-clock delay before cancelling. Each
//! scripted peer records the exact protocol stage it reached and signals it
//! over a [`tokio::sync::oneshot`] channel (only after the bytes have been
//! flushed onto the socket). The test awaits that signal and only then cancels
//! the session, so what is under test is a *protocol state*, never an elapsed
//! duration. A bounded timeout turns a peer that never reaches the stage into a
//! clear failure instead of a hang.

mod support;

use ferrisync_core::protocol::{FileChunk, SyncMessage};
use ferrisync_core::safe_fs::TEMP_PREFIX;
use ferrisync_core::sync_engine::server::{serve_folder, PairPolicy};
use ferrisync_core::DeviceInfo;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use support::*;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;

/// The payload the scripted peers advertise throughout this file. 256 KiB is
/// larger than one 64 KiB chunk but stays well inside the in-flight buffer cap,
/// so "after multiple chunks" is exercised without tripping a resource limit.
fn payload() -> Vec<u8> {
    (0..256 * 1024).map(|i| (i % 251) as u8).collect()
}

/// The largest chunk the initiator will emit (`CHUNK_SIZE` in `session.rs`).
/// A partial delivery must be smaller than this to guarantee it is incomplete.
const ONE_CHUNK: usize = 64 * 1024;

/// Upper bound for how long a test waits for a scripted peer to announce that
/// it reached the stage under test. Generous enough for a loaded CI box, but
/// finite so a broken script fails loudly instead of hanging.
const SYNC_TIMEOUT: Duration = Duration::from_secs(30);

fn hash_of(data: &[u8]) -> Vec<u8> {
    blake3::hash(data).as_bytes().to_vec()
}

/// Every `TEMP_PREFIX`-named artifact currently in `dir`.
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

/// Session rows recorded for this side, newest first.
fn session_rows(a: &TestSide) -> Vec<ferrisync_core::storage::SessionRecord> {
    a.storage.list_recent_sessions(16).unwrap()
}

fn completed_sessions(a: &TestSide) -> usize {
    session_rows(a)
        .iter()
        .filter(|s| s.status == "complete")
        .count()
}

/// Local file-history rows (each successful push/pull records exactly one).
fn history_rows(a: &TestSide) -> Vec<ferrisync_core::storage::FileHistoryRow> {
    a.storage.list_file_history(None, 64).unwrap()
}

fn history_count(a: &TestSide, path: &str) -> usize {
    history_rows(a).iter().filter(|r| r.path == path).count()
}

/// Which protocol stage a scripted peer reached, reported once observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// A file transfer was received but deliberately left unfinished.
    HeldPartial,
    /// The complete file has been written to the socket, but no further frames
    /// (including the peer's own trailing messages) have been sent.
    FullFileSent,
    /// No acknowledgement was ever sent for the client's upload.
    HeldUnacked,
    /// Teardown occurred while a transfer was still in progress.
    Disconnected,
}

/// Await the peer's report that it reached the stage under test, failing
/// loudly on timeout.
async fn wait_for_stage(rx: oneshot::Receiver<Stage>, what: &str) -> Stage {
    tokio::time::timeout(SYNC_TIMEOUT, rx)
        .await
        .unwrap_or_else(|_| panic!("peer never reached the {what} stage (timed out)"))
        .unwrap_or_else(|_| panic!("peer dropped before reaching the {what} stage"))
}

/// Read one message frame from the client, returning `None` on EOF, a non-chunk
/// message, or a malformed frame.
async fn recv_message(stream: &mut PeerStream) -> Option<SyncMessage> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await.ok()?;
    let len = u32::from_be_bytes(header) as usize;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await.ok()?;
    let framed: Vec<u8> = header.iter().copied().chain(body).collect();
    ferrisync_core::protocol::parse_frame(&framed)
        .ok()
        .map(|(m, _)| m)
}

/// Drain the client's handshake chatter (Hello, Index, sentinel Ack) until the
/// first `FileChunk` arrives, returning it.
async fn recv_next_chunk(stream: &mut PeerStream) -> Option<FileChunk> {
    loop {
        match recv_message(stream).await? {
            SyncMessage::FileChunk(chunk) => return Some(chunk),
            SyncMessage::Hello(_) | SyncMessage::Index(_) | SyncMessage::Ack(_) => continue,
            _ => return None,
        }
    }
}

/// Build an index announcing a single file the initiator should download.
fn download_index(device_id: &str, path: &str, payload: &[u8]) -> ferrisync_core::protocol::Index {
    let mut index = empty_index(device_id);
    index
        .entries
        .push(index_entry(path, payload.len() as u64, &hash_of(payload)));
    index
}

/// Spawn a scripted peer that announces `path` (as `payload`), sends
/// `delivered` bytes of it, reports [`Stage::HeldPartial`], then holds the
/// connection open without ever completing the file.
async fn spawn_partial_download(
    crypto: Arc<ferrisync_core::crypto::CryptoProvider>,
    device_id: &str,
    path: &str,
    payload: &[u8],
    delivered: Vec<u8>,
) -> (std::net::SocketAddr, oneshot::Receiver<Stage>) {
    let index = download_index(device_id, path, payload);
    let path = path.to_string();
    let total = payload.len() as u64;
    let (stage_tx, stage_rx) = oneshot::channel();
    let addr = spawn_scripted_peer(crypto, index, move |mut stream| async move {
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path,
                offset: 0,
                data: delivered,
                total_size: total,
            }),
        )
        .await;
        // The bytes are on the wire now; the initiator will drain them and block
        // waiting for the rest. Only now do we announce the stage.
        let _ = stage_tx.send(Stage::HeldPartial);
        hold_open(stream).await;
    })
    .await;
    (addr, stage_rx)
}

/// Spawn a scripted peer that announces `path` (as `payload`) but never sends
/// any of it, reporting [`Stage::HeldPartial`] immediately after the handshake.
async fn spawn_withheld_download(
    crypto: Arc<ferrisync_core::crypto::CryptoProvider>,
    device_id: &str,
    path: &str,
    payload: &[u8],
) -> (std::net::SocketAddr, oneshot::Receiver<Stage>) {
    let index = download_index(device_id, path, payload);
    let (stage_tx, stage_rx) = oneshot::channel();
    let addr = spawn_scripted_peer(crypto, index, move |stream| async move {
        let _ = stage_tx.send(Stage::HeldPartial);
        hold_open(stream).await;
    })
    .await;
    (addr, stage_rx)
}

/// Spawn a scripted peer with an empty index that never acknowledges the
/// client's upload, reporting [`Stage::HeldUnacked`] only once those upload
/// bytes have actually arrived (so cancellation genuinely lands mid-upload).
async fn spawn_upload_stall(
    crypto: Arc<ferrisync_core::crypto::CryptoProvider>,
    device_id: &str,
    chunks_before_report: usize,
) -> (std::net::SocketAddr, oneshot::Receiver<Stage>) {
    let (stage_tx, stage_rx) = oneshot::channel();
    let addr = spawn_scripted_peer(
        crypto,
        empty_index(device_id),
        move |mut stream| async move {
            for _ in 0..chunks_before_report {
                if recv_next_chunk(&mut stream).await.is_none() {
                    return;
                }
            }
            let _ = stage_tx.send(Stage::HeldUnacked);
            hold_open(stream).await;
        },
    )
    .await;
    (addr, stage_rx)
}

/// A peer that receives the upload but closes the connection mid-transfer,
/// reporting [`Stage::Disconnected`] before it tears down.
async fn spawn_disconnecting_upload_peer(
    crypto: Arc<ferrisync_core::crypto::CryptoProvider>,
    device_id: &str,
    chunks_before_close: usize,
) -> (std::net::SocketAddr, oneshot::Receiver<Stage>) {
    let (stage_tx, stage_rx) = oneshot::channel();
    let addr = spawn_scripted_peer(
        crypto,
        empty_index(device_id),
        move |mut stream| async move {
            for _ in 0..chunks_before_close {
                if recv_next_chunk(&mut stream).await.is_none() {
                    return;
                }
            }
            let _ = stage_tx.send(Stage::Disconnected);
            // Dropping the TLS stream closes the TCP connection.
        },
    )
    .await;
    (addr, stage_rx)
}

/// A peer that fully receives the upload, reports [`Stage::Disconnected`] after
/// the last byte, then closes without acknowledging — a transport failure that
/// lands exactly at the commit boundary.
async fn spawn_disconnect_after_full_upload(
    crypto: Arc<ferrisync_core::crypto::CryptoProvider>,
    device_id: &str,
    total_chunks: usize,
) -> (std::net::SocketAddr, oneshot::Receiver<Stage>) {
    let (stage_tx, stage_rx) = oneshot::channel();
    let addr = spawn_scripted_peer(
        crypto,
        empty_index(device_id),
        move |mut stream| async move {
            for _ in 0..total_chunks {
                if recv_next_chunk(&mut stream).await.is_none() {
                    return;
                }
            }
            let _ = stage_tx.send(Stage::Disconnected);
            // Dropping the TLS stream closes the TCP connection.
        },
    )
    .await;
    (addr, stage_rx)
}

/// Keep the TLS connection open (dropping it would surface as a broken pipe)
/// while sending nothing, so the initiator blocks on its next read. The stream
/// is kept alive for the lifetime of the future.
async fn hold_open(_stream: PeerStream) {
    std::future::pending::<()>().await;
}

/// Spawn a scripted peer that announces `path` (as `payload`) and writes the
/// *entire* file, then reports [`Stage::FullFileSent`] and holds the connection
/// open. Nothing after the last chunk is sent, so this pins the initiator to the
/// exact boundary where all bytes have arrived but finalization/ack has not
/// necessarily happened — the point at which a cancellation is most dangerous.
async fn spawn_full_download_then_hold(
    crypto: Arc<ferrisync_core::crypto::CryptoProvider>,
    device_id: &str,
    path: &str,
    payload: &[u8],
) -> (std::net::SocketAddr, oneshot::Receiver<Stage>) {
    let index = download_index(device_id, path, payload);
    let path = path.to_string();
    let total = payload.len() as u64;
    let payload = payload.to_vec();
    let (stage_tx, stage_rx) = oneshot::channel();
    let addr = spawn_scripted_peer(crypto, index, move |mut stream| async move {
        let mut offset = 0usize;
        while offset < payload.len() {
            let end = (offset + ONE_CHUNK).min(payload.len());
            let chunk = FileChunk {
                folder_id: "1".into(),
                path: path.clone(),
                offset: offset as u64,
                data: payload[offset..end].to_vec(),
                total_size: total,
            };
            if write_msg(&mut stream, &SyncMessage::FileChunk(chunk))
                .await
                .is_err()
            {
                return;
            }
            offset = end;
        }
        // The whole file is on the wire; the initiator may be anywhere between
        // "reading the last chunk" and "having committed it".
        let _ = stage_tx.send(Stage::FullFileSent);
        hold_open(stream).await;
    })
    .await;
    (addr, stage_rx)
}

// ────────────────────────────────────────────────────────────────────
// Phase 1/2 — download cancellation at several transfer stages
// ────────────────────────────────────────────────────────────────────

/// Run a session in the background and cancel it *as soon as* `stage_rx`
/// reports that the peer reached the protocol state under test. Returns the
/// join result of the cancelled task once it has fully terminated, so callers
/// can inspect cleanup afterwards.
async fn cancel_when_peer_reports(
    a: &TestSide,
    addr: std::net::SocketAddr,
    peer_id: &str,
    stage_rx: oneshot::Receiver<Stage>,
    what: &str,
) -> Result<Result<ferrisync_core::SyncResult, anyhow::Error>, tokio::task::JoinError> {
    let handle = tokio::spawn(run_client_owned(
        a.crypto.clone(),
        a.storage.clone(),
        a.path().to_string(),
        addr,
        a.folder_id,
        peer_id.to_string(),
    ));
    // Wait for the peer to confirm the exact protocol stage; only then cancel.
    wait_for_stage(stage_rx, what).await;
    handle.abort();
    handle.await
}

/// Identify whether a finished session task failed because it was aborted.
fn assert_was_cancelled<T: std::fmt::Debug>(
    joined: &Result<T, tokio::task::JoinError>,
    what: &str,
) {
    let err = joined.as_ref().expect_err(&format!(
        "{what}: the session must have been cancelled, not completed"
    ));
    assert!(
        err.is_cancelled(),
        "{what}: expected a cancellation, got {err}"
    );
}

/// Run a session in the background, wait until `stage_rx` reports that the peer
/// reached the state under test, then *let the session finish on its own* and
/// return what `run_sync_session` produced. Used for peer-side failures (the
/// peer tears the connection down) rather than caller-initiated cancellation.
async fn run_session_until_peer_reports(
    a: &TestSide,
    addr: std::net::SocketAddr,
    peer_id: &str,
    stage_rx: oneshot::Receiver<Stage>,
    what: &str,
) -> anyhow::Result<ferrisync_core::SyncResult> {
    let handle = tokio::spawn(run_client_owned(
        a.crypto.clone(),
        a.storage.clone(),
        a.path().to_string(),
        addr,
        a.folder_id,
        peer_id.to_string(),
    ));
    wait_for_stage(stage_rx, what).await;
    // Do not abort: the peer has already closed its side, so the session must
    // observe that failure and terminate by itself (bounded by SESSION_TIMEOUT).
    handle
        .await
        .expect("the session task must terminate on its own after the peer fails")
}

/// Cancel before any payload arrives: nothing may be committed, and the peer's
/// incomplete transfer must not leave an artifact.
#[tokio::test]
async fn cancel_before_payload_leaves_no_destination() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let want = payload();

    let (addr, stage_rx) =
        spawn_withheld_download(fresh_crypto(), &id_b, "before.bin", &want).await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-before-payload").await;
    assert_was_cancelled(&joined, "cancel-before-payload");
    assert!(
        !a.dir.path().join("before.bin").exists(),
        "no destination may exist when the payload never arrived"
    );
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(completed_sessions(&a), 0);
}

/// Cancel after a partial payload: a single sub-chunk delivery must never be
/// committed as a valid file.
#[tokio::test]
async fn cancel_after_partial_payload_leaves_no_destination() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let want = payload();

    let (addr, stage_rx) = spawn_partial_download(
        fresh_crypto(),
        &id_b,
        "partial.bin",
        &want,
        want[..ONE_CHUNK / 2].to_vec(),
    )
    .await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-partial").await;
    assert_was_cancelled(&joined, "cancel-after-partial");

    assert!(
        !a.dir.path().join("partial.bin").exists(),
        "an incomplete download must never be committed"
    );
    assert!(
        stranded_temp_files(a.dir.path()).is_empty(),
        "cancelling must not strand a temporary file: {:?}",
        stranded_temp_files(a.dir.path())
    );
    assert_eq!(completed_sessions(&a), 0);
}

/// Cancel after several chunks: still no committed (or corrupt) destination.
#[tokio::test]
async fn cancel_after_multiple_chunks_leaves_no_destination() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let want = payload();

    let (addr, stage_rx) = spawn_partial_download(
        fresh_crypto(),
        &id_b,
        "multi.bin",
        &want,
        want[..2 * ONE_CHUNK + 7].to_vec(),
    )
    .await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-multi-chunk").await;
    assert_was_cancelled(&joined, "cancel-after-multiple-chunks");

    assert!(
        !a.dir.path().join("multi.bin").exists(),
        "a multi-chunk partial must never be committed"
    );
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(completed_sessions(&a), 0);
}

/// Cancel while a destination already exists: the sentinel bytes must survive
/// unchanged, because a partial download is never committed over them.
#[tokio::test]
async fn cancel_while_replacing_preserves_existing_destination() {
    const SENTINEL: &[u8] = b"PRIOR-VERSION-MUST-SURVIVE";
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let want = payload();
    a.write("keep.bin", SENTINEL);

    // The remote version dominates so the initiator plans a download over the
    // existing local copy.
    let mut index = empty_index(&id_b);
    index.entries.push(index_entry_remote_wins(
        "keep.bin",
        want.len() as u64,
        &hash_of(&want),
    ));

    let (stage_tx, stage_rx) = oneshot::channel();
    let delivered = want[..ONE_CHUNK / 2].to_vec();
    let total = want.len() as u64;
    let addr = spawn_scripted_peer(fresh_crypto(), index, move |mut stream| async move {
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path: "keep.bin".into(),
                offset: 0,
                data: delivered,
                total_size: total,
            }),
        )
        .await;
        let _ = stage_tx.send(Stage::HeldPartial);
        hold_open(stream).await;
    })
    .await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-replace").await;
    assert_was_cancelled(&joined, "cancel-while-replacing");

    assert_eq!(
        a.read("keep.bin"),
        SENTINEL,
        "a cancellation before commit must leave the previous destination intact"
    );
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    // Exactly one file in the root: the untouched sentinel.
    let entries: Vec<_> = std::fs::read_dir(a.dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "metadata.db")
        .collect();
    assert_eq!(
        entries,
        vec!["keep.bin".to_string()],
        "unexpected artifacts: {entries:?}"
    );
}

/// Cancel at the finalization boundary: the peer has delivered every byte but
/// the initiator may not yet have committed. The atomic-commit contract allows
/// two outcomes and only two — the destination is either absent or the complete,
/// correct file. A partial or corrupt destination (or a stranded temp file) is a
/// contract violation.
#[tokio::test]
async fn cancel_at_finalization_boundary_is_atomic() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let want = payload();

    let (addr, stage_rx) =
        spawn_full_download_then_hold(fresh_crypto(), &id_b, "final.bin", &want).await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "full-file-sent").await;
    assert_was_cancelled(&joined, "cancel-at-finalization");

    let dest = a.dir.path().join("final.bin");
    if dest.exists() {
        assert_eq!(
            a.read("final.bin"),
            want,
            "if the destination exists it must be the complete, correct file"
        );
    }
    assert!(
        stranded_temp_files(a.dir.path()).is_empty(),
        "cancellation at the commit boundary must not strand a temp file: {:?}",
        stranded_temp_files(a.dir.path())
    );
    assert_eq!(completed_sessions(&a), 0);
}

// ────────────────────────────────────────────────────────────────────
// Phase 3 — upload cancellation
// ────────────────────────────────────────────────────────────────────

/// Cancel an upload before the peer has received any chunk: the source is
/// unchanged and no local artifact remains.
#[tokio::test]
async fn cancel_upload_before_first_chunk_leaves_source_clean() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let source = payload();
    a.write("up.bin", &source);
    let original_len = std::fs::metadata(a.dir.path().join("up.bin"))
        .unwrap()
        .len();

    let (addr, stage_rx) = spawn_upload_stall(fresh_crypto(), &id_b, 0).await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-unacked").await;
    assert_was_cancelled(&joined, "cancel-upload-before-first-chunk");

    assert_eq!(a.read("up.bin"), source, "the source must be unchanged");
    assert_eq!(
        std::fs::metadata(a.dir.path().join("up.bin"))
            .unwrap()
            .len(),
        original_len
    );
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(completed_sessions(&a), 0);
}

/// Cancel an upload after the peer has received at least one chunk but before
/// it acknowledges: the source is still byte-for-byte intact.
#[tokio::test]
async fn cancel_upload_after_partial_receipt_leaves_source_clean() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let source = payload();
    a.write("up.bin", &source);

    let (addr, stage_rx) = spawn_upload_stall(fresh_crypto(), &id_b, 1).await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-after-one-chunk").await;
    assert_was_cancelled(&joined, "cancel-upload-after-partial-receipt");

    assert_eq!(a.read("up.bin"), source, "the source must be unchanged");
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    // This side never recorded a completion, and the local push history is not
    // written until the upload is acknowledged.
    assert_eq!(completed_sessions(&a), 0);
    assert_eq!(
        history_count(&a, "up.bin"),
        0,
        "an unacknowledged upload must not be recorded as a completed push"
    );
}

// ────────────────────────────────────────────────────────────────────
// Phase 4 — retry and resource release
// ────────────────────────────────────────────────────────────────────

/// The core recovery promise: after a cancelled download, the very next
/// session delivers the complete file, leaves no artifact, and is the only one
/// recorded as complete.
#[tokio::test]
async fn session_can_be_retried_after_a_cancellation() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");
    let want = payload();
    b.write("recover.bin", &want);

    // First attempt: cancelled mid-download against a peer that stalls. It uses
    // the same identity as the real server below, so the initiator's
    // trust-on-first-use binding from this attempt still matches on retry.
    let (addr, stage_rx) = spawn_partial_download(
        b.crypto.clone(),
        &id_b,
        "recover.bin",
        &want,
        want[..ONE_CHUNK / 2].to_vec(),
    )
    .await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-partial").await;
    assert_was_cancelled(&joined, "retry-first-attempt");
    assert_eq!(
        completed_sessions(&a),
        0,
        "the cancelled attempt must not be recorded"
    );

    // Retry against the real server: the cancelled session must not have left
    // the client's storage or root in a state that blocks recovery.
    let addr = spawn_real_server(&b, &id_b).await;
    let result = run_client(&a, addr, &id_b)
        .await
        .expect("a session must be retryable after a cancellation");
    assert!(result.pulled.contains(&"recover.bin".to_string()));
    assert_eq!(
        a.read("recover.bin"),
        want,
        "the retry must deliver the whole file"
    );
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(
        completed_sessions(&a),
        1,
        "the successful retry must be recorded exactly once"
    );
    assert_eq!(
        history_count(&a, "recover.bin"),
        1,
        "exactly one pull recorded"
    );
}

/// Retry through the production serving path (`serve_folder`), proving the
/// destination lock table does not stay held after a cancelled session.
#[tokio::test]
async fn real_server_session_succeeds_after_a_cancellation() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");
    let want = payload();
    b.write("served.bin", &want);

    // The production server only syncs with a *known* device: register the
    // client's certificate on the server's device table (the same TOFU binding
    // a real pairing would establish) so the retry is authorized. This is what
    // makes the test exercise the production authorization path rather than the
    // test-only handler.
    let client_cert = a.crypto.certificate().await;
    let client_derived_id = ferrisync_core::crypto::cert_to_device_id(&client_cert);
    b.storage
        .set_device_cert(&client_derived_id, &client_cert)
        .unwrap();

    // Bind the real server on an ephemeral port, then point the stalling peer
    // at the same identity so TOFU keeps matching across the retry.
    let (handle, mut events) = serve_folder(
        b.storage.clone(),
        b.crypto.clone(),
        DeviceInfo {
            id: id_a.clone(),
            name: "server-b".into(),
            cert_fingerprint: vec![],
        },
        b.path().to_string(),
        0,
        PairPolicy::AutoAccept,
        dummy_store(),
    )
    .await
    .expect("serve_folder must bind");
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", handle.port).parse().unwrap();
    // Drain events in the background so the server's bounded channel never
    // blocks a session.
    let drain = tokio::spawn(async move { while events.recv().await.is_some() {} });

    let (stall, stage_rx) = spawn_partial_download(
        b.crypto.clone(),
        &id_b,
        "served.bin",
        &want,
        want[..ONE_CHUNK / 2].to_vec(),
    )
    .await;
    let joined = cancel_when_peer_reports(&a, stall, &id_b, stage_rx, "held-partial").await;
    assert_was_cancelled(&joined, "real-server-first-attempt");

    // Retry against the production server.
    let result = run_client(&a, addr, &id_b)
        .await
        .expect("the production server must accept the retry after a cancellation");
    assert!(result.pulled.contains(&"served.bin".to_string()));
    assert_eq!(a.read("served.bin"), want);
    assert_eq!(completed_sessions(&a), 1);

    drain.abort();
    handle.stop().await;
}

/// Repeated cancel/retry cycles must not accumulate destinations, temporary
/// files, or completion records: the Nth retry behaves like the first.
#[tokio::test]
async fn repeated_cancellation_does_not_accumulate_state() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");
    let want = payload();
    b.write("cycle.bin", &want);

    for round in 0..3 {
        let (addr, stage_rx) = spawn_partial_download(
            b.crypto.clone(),
            &id_b,
            "cycle.bin",
            &want,
            want[..ONE_CHUNK / 2].to_vec(),
        )
        .await;
        let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-partial").await;
        assert_was_cancelled(&joined, &format!("repeated-cancellation round {round}"));

        // No leftover state between rounds.
        assert!(
            !a.dir.path().join("cycle.bin").exists(),
            "round {round}: a partial download must not be committed"
        );
        assert!(
            stranded_temp_files(a.dir.path()).is_empty(),
            "round {round}: a cancelled session must not strand a temp file"
        );
        assert_eq!(
            completed_sessions(&a),
            0,
            "round {round}: a cancelled session must not be recorded as complete"
        );
    }

    // One real session at the end proves nothing accumulated across the cycles.
    let addr = spawn_real_server(&b, &id_b).await;
    let result = run_client(&a, addr, &id_b)
        .await
        .expect("a session must succeed after repeated cancellations");
    assert!(result.pulled.contains(&"cycle.bin".to_string()));
    assert_eq!(a.read("cycle.bin"), want);
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(
        completed_sessions(&a),
        1,
        "only the final successful session may be recorded as complete"
    );
}

// ────────────────────────────────────────────────────────────────────
// Phase 5 — disconnect and failure paths
// ────────────────────────────────────────────────────────────────────

/// Recovery after more than one failed attempt: two consecutive transport
/// failures, then a real session that succeeds — proving neither failed attempt
/// leaves state that blocks or corrupts the eventual sync.
#[tokio::test]
async fn two_failed_attempts_then_success_recovers() {
    let id_a = uuid::Uuid::new_v4().to_string();
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let b = TestSide::new(&id_a, "server-b");
    let want = payload();
    b.write("recover2.bin", &want);

    // Two failed attempts, each failing a different way: a mid-download stall
    // (cancelled) and a mid-upload disconnect (failure).
    let (addr, stage_rx) = spawn_partial_download(
        b.crypto.clone(),
        &id_b,
        "recover2.bin",
        &want,
        want[..ONE_CHUNK / 2].to_vec(),
    )
    .await;
    let joined = cancel_when_peer_reports(&a, addr, &id_b, stage_rx, "held-partial").await;
    assert_was_cancelled(&joined, "recover2-first-attempt");

    a.write("side.bin", &want);
    let (addr, stage_rx) = spawn_disconnecting_upload_peer(b.crypto.clone(), &id_b, 1).await;
    let second =
        run_session_until_peer_reports(&a, addr, &id_b, stage_rx, "disconnected-upload").await;
    assert!(
        second.is_err(),
        "the second attempt must fail on the peer disconnect"
    );

    assert_eq!(completed_sessions(&a), 0, "neither failure may be recorded");

    // Third attempt: a real session must recover completely.
    let addr = spawn_real_server(&b, &id_b).await;
    let result = run_client(&a, addr, &id_b)
        .await
        .expect("a session must succeed after two failed attempts");
    assert!(result.pulled.contains(&"recover2.bin".to_string()));
    assert_eq!(
        a.read("recover2.bin"),
        want,
        "the retry must deliver the file"
    );
    assert_eq!(a.read("side.bin"), want, "the local source must be intact");
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(
        completed_sessions(&a),
        1,
        "only the final successful session may be recorded as complete"
    );
}

/// A peer that disconnects mid-upload: the session fails, but the local source
/// is preserved and no artifact is left behind.
#[tokio::test]
async fn disconnect_during_upload_preserves_source() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let source = payload();
    a.write("gone.bin", &source);

    let (addr, stage_rx) = spawn_disconnecting_upload_peer(fresh_crypto(), &id_b, 1).await;
    let result =
        run_session_until_peer_reports(&a, addr, &id_b, stage_rx, "disconnected-upload").await;
    assert!(
        result.is_err(),
        "a peer that disconnects mid-upload must fail the session"
    );
    assert_eq!(a.read("gone.bin"), source, "the source must be preserved");
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(completed_sessions(&a), 0);
}

/// A disconnect landing exactly after the full upload but before the ack: this
/// is the boundary between "committed" and "verified", and it must never be
/// reported as a completed push.
#[tokio::test]
async fn disconnect_after_full_upload_before_ack_is_not_complete() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    let source = payload();
    a.write("boundary.bin", &source);
    // The 256 KiB payload is sent as exactly four 64 KiB chunks.
    let total_chunks = source.len().div_ceil(ONE_CHUNK);

    let (addr, stage_rx) =
        spawn_disconnect_after_full_upload(fresh_crypto(), &id_b, total_chunks).await;
    let result =
        run_session_until_peer_reports(&a, addr, &id_b, stage_rx, "disconnected-after-full").await;
    assert!(
        result.is_err(),
        "a disconnect before the ack must not be reported as success"
    );
    assert_eq!(
        a.read("boundary.bin"),
        source,
        "the source must be preserved"
    );
    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(
        completed_sessions(&a),
        0,
        "an unacknowledged (unverified) push must not be recorded as complete"
    );
    assert_eq!(
        history_count(&a, "boundary.bin"),
        0,
        "an unacknowledged push must not be recorded in history"
    );
}

/// Cancelling before the handshake finishes must release cleanly: the peer gets
/// torn down and nothing is committed locally.
#[tokio::test]
async fn cancel_before_handshake_completes_cleans_up() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    // A letter box that accepts the TCP connection but never completes TLS, so
    // the client is stuck inside its handshake.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (accepted_tx, accepted_rx) = oneshot::channel();
    let peer = tokio::spawn(async move {
        if listener.accept().await.is_ok() {
            let _ = accepted_tx.send(Stage::HeldUnacked);
            std::future::pending::<()>().await;
        }
    });

    let handle = tokio::spawn(run_client_owned(
        a.crypto.clone(),
        a.storage.clone(),
        a.path().to_string(),
        addr,
        a.folder_id,
        id_b.clone(),
    ));
    assert_eq!(
        wait_for_stage(accepted_rx, "tcp-accepted").await,
        Stage::HeldUnacked,
        "the client must have connected before we cancel"
    );
    // Cancel while the handshake is in flight.
    handle.abort();
    let joined = handle.await;
    assert_was_cancelled(&joined, "cancel-before-handshake");

    assert!(stranded_temp_files(a.dir.path()).is_empty());
    assert_eq!(completed_sessions(&a), 0);
    peer.abort();
}

// ────────────────────────────────────────────────────────────────────
// Helpers shared with other suites
// ────────────────────────────────────────────────────────────────────

/// A fresh, unpaired identity for a scripted peer that never reconnects.
fn fresh_crypto() -> Arc<ferrisync_core::crypto::CryptoProvider> {
    Arc::new(ferrisync_core::crypto::CryptoProvider::generate().unwrap())
}

/// Like [`run_client`], but owns everything it needs so it can be spawned onto a
/// task and cancelled. Returns the session result as `anyhow::Result`.
async fn run_client_owned(
    crypto: Arc<ferrisync_core::crypto::CryptoProvider>,
    storage: Arc<ferrisync_core::storage::Storage>,
    local_path: String,
    addr: std::net::SocketAddr,
    folder_id: i64,
    peer_id: String,
) -> anyhow::Result<ferrisync_core::SyncResult> {
    let (tx, _rx) = tokio::sync::mpsc::channel(256);
    ferrisync_core::sync_engine::session::run_sync_session(
        crypto,
        storage,
        &local_path,
        addr,
        folder_id,
        &peer_id,
        tx,
        dummy_store(),
        false,
        None,
    )
    .await
}

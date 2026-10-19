//! Milestone 4: adversarial inbound-transfer tests.
//!
//! Every test drives the real `run_sync_session` against a hand-scripted peer
//! so the exact byte sequence on the wire is under test control — no timing,
//! no external network. They assert that a hostile or broken peer can never
//! turn a malformed session into a success or a filesystem write.

mod support;

use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::protocol::{FileChunk, SyncMessage};
use std::sync::Arc;
use std::time::Duration;
use support::*;

const PAYLOAD: &[u8] = b"payload";

/// A peer that completes TLS and then vanishes before the sync handshake must
/// not yield success, and must leave the local files untouched.
#[tokio::test]
async fn disconnect_during_handshake_is_not_success() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    a.write("f.txt", PAYLOAD);

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    // TLS completes, then the socket is dropped without a Hello.
    let addr = spawn_raw_peer(b_crypto, |_stream| async {}).await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(
        result.is_err(),
        "a peer that never completes the handshake must not yield success"
    );
    assert_eq!(
        a.read("f.txt"),
        PAYLOAD,
        "the source file must be untouched"
    );
}

/// A connection that ends in the middle of a data frame (EOF before the
/// promised bytes arrive) must fail the session, not silently succeed.
#[tokio::test]
async fn eof_mid_chunk_is_not_success() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let mut index = empty_index(&id_b);
    // We expect to receive this path, so the client stays in its receive loop.
    index
        .entries
        .push(index_entry("want.txt", 5, blake3::hash(PAYLOAD).as_bytes()));

    let addr = spawn_scripted_peer(b_crypto, index, |mut stream| async move {
        use tokio::io::AsyncWriteExt;
        // Promise a 1024-byte frame, deliver three bytes, then close.
        let mut partial = Vec::new();
        partial.extend_from_slice(&1024u32.to_be_bytes());
        partial.extend_from_slice(&[0u8; 3]);
        let _ = stream.write_all(&partial).await;
        let _ = stream.shutdown().await;
    })
    .await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(
        result.is_err(),
        "a truncated data frame must fail the session"
    );
    assert!(
        !a.dir.path().join("want.txt").exists(),
        "no partial file may be committed"
    );
}

/// A chunk that declares a different total size than the index agreed must be
/// rejected before any bytes reach the filesystem.
#[tokio::test]
async fn declared_size_mismatch_is_rejected() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let mut index = empty_index(&id_b);
    index
        .entries
        .push(index_entry("want.txt", 5, blake3::hash(PAYLOAD).as_bytes()));

    let addr = spawn_scripted_peer(b_crypto, index, |mut stream| async move {
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path: "want.txt".into(),
                offset: 0,
                data: PAYLOAD.to_vec(),
                // The index says 5; the chunk lies and says 6.
                total_size: 6,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(
        result.is_err(),
        "a declared-size mismatch must fail the session"
    );
    assert!(!a.dir.path().join("want.txt").exists());
}

/// A chunk that extends past the declared total size must be rejected.
#[tokio::test]
async fn chunk_past_total_size_is_rejected() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let mut index = empty_index(&id_b);
    index
        .entries
        .push(index_entry("want.txt", 5, blake3::hash(PAYLOAD).as_bytes()));

    let addr = spawn_scripted_peer(b_crypto, index, |mut stream| async move {
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path: "want.txt".into(),
                offset: 0,
                data: b"way-too-much-data".to_vec(),
                total_size: 5,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(result.is_err(), "an over-long chunk must fail the session");
    assert!(!a.dir.path().join("want.txt").exists());
}

/// Chunks must arrive contiguous and in order; a gap must fail the session.
#[tokio::test]
async fn out_of_order_chunk_is_rejected() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let data = b"0123456789";
    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let mut index = empty_index(&id_b);
    index
        .entries
        .push(index_entry("want.bin", 10, blake3::hash(data).as_bytes()));

    let addr = spawn_scripted_peer(b_crypto, index, move |mut stream| async move {
        // First chunk covers [0,4) …
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path: "want.bin".into(),
                offset: 0,
                data: data[0..4].to_vec(),
                total_size: 10,
            }),
        )
        .await;
        // … then a chunk at offset 6 leaves a gap at [4,6).
        let _ = write_msg(
            &mut stream,
            &SyncMessage::FileChunk(FileChunk {
                folder_id: "1".into(),
                path: "want.bin".into(),
                offset: 6,
                data: data[6..10].to_vec(),
                total_size: 10,
            }),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let result = run_client(&a, addr, &id_b).await;
    assert!(result.is_err(), "a chunk gap must fail the session");
    assert!(!a.dir.path().join("want.bin").exists());
}

/// A remote index that lists the same path twice is malformed and must be
/// rejected before it drives any transfer.
#[tokio::test]
async fn duplicate_index_entries_are_rejected() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let mut index = empty_index(&id_b);
    index
        .entries
        .push(index_entry("dup.txt", 5, blake3::hash(PAYLOAD).as_bytes()));
    index
        .entries
        .push(index_entry("dup.txt", 5, blake3::hash(PAYLOAD).as_bytes()));

    let addr = spawn_scripted_peer(b_crypto, index, |_stream| async {}).await;
    let result = run_client(&a, addr, &id_b).await;

    assert!(
        result.is_err(),
        "a duplicate entry in the remote index must be rejected"
    );
    assert!(!a.dir.path().join("dup.txt").exists());
}

/// An absolute path in the remote index must be rejected and never written.
#[tokio::test]
async fn absolute_path_in_index_is_rejected() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");

    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());
    let mut index = empty_index(&id_b);
    index
        .entries
        .push(index_entry("/etc/ferrisync-pwned", 5, &[0u8; 32]));

    let addr = spawn_scripted_peer(b_crypto, index, |_stream| async {}).await;
    let result = run_client(&a, addr, &id_b).await;

    assert!(
        result.is_err(),
        "an absolute path in the remote index must be rejected"
    );
    assert!(!std::path::Path::new("/etc/ferrisync-pwned").exists());
}

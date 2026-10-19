//! Milestone 3/4: snapshot consistency at the session level.
//!
//! Deterministic end-to-end proof that a file which changes between indexing
//! and upload is not transmitted as if it still matched the advertised index,
//! and that the session is reported incomplete rather than successful.

mod support;

use ferrisync_core::crypto::CryptoProvider;
use std::sync::Arc;
use std::time::Duration;
use support::*;

#[tokio::test]
async fn file_changed_after_indexing_is_skipped_not_transmitted() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let a = TestSide::new(&id_b, "client-a");
    // The byte count and content the client will advertise in its index.
    a.write("f.txt", b"ORIGINAL-CONTENT");

    let client_dir = a.dir.path().to_path_buf();
    let b_crypto = Arc::new(CryptoProvider::generate().unwrap());

    // The hook runs after the client has sent its index (its scanned snapshot)
    // but before its upload, so the file on disk no longer matches what was
    // indexed. This is the exact race Step 3.1 closes.
    let addr = spawn_scripted_peer_with_hook(
        b_crypto,
        empty_index(&id_b),
        move || async move {
            tokio::fs::write(client_dir.join("f.txt"), b"CHANGED-AFTER-INDEX")
                .await
                .unwrap();
        },
        |_stream| async {
            tokio::time::sleep(Duration::from_millis(300)).await;
        },
    )
    .await;

    let result = run_client(&a, addr, &id_b).await;

    // The stale upload must not be counted as a success.
    assert!(
        result.is_err(),
        "a file that changed after indexing must not yield a successful session"
    );
    assert_eq!(
        a.read("f.txt"),
        b"CHANGED-AFTER-INDEX",
        "the source file must be left exactly as the user changed it"
    );

    // The session is recorded as incomplete, not complete.
    let sessions = a.storage.list_recent_sessions(1).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].status, "incomplete",
        "a session with a skipped upload must be recorded incomplete"
    );
}

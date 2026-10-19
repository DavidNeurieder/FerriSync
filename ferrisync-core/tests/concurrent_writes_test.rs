//! Milestone 4: concurrency tests.
//!
//! Simultaneous sessions that target the same destination file must never
//! produce a corrupt or blended file: whichever write lands last, the
//! destination must be exactly one whole version and no temporary file may be
//! left behind.

mod support;

use support::*;

fn payload1() -> Vec<u8> {
    vec![b'A'; 64 * 1024]
}

fn payload2() -> Vec<u8> {
    vec![b'B'; 96 * 1024]
}

/// Two clients push different versions of the same path to one server at the
/// same time. The server's destination must end up as one whole payload.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_sessions_to_one_destination_never_corrupt() {
    let id_b = uuid::Uuid::new_v4().to_string();
    let server = TestSide::new(&id_b, "server-b");

    let addr = spawn_multi_server(
        server.crypto.clone(),
        server.storage.clone(),
        server.path().to_string(),
        server.folder_id,
        id_b.clone(),
    )
    .await;

    // Two clients with different content for the same destination path.
    let client1 = TestSide::new(&id_b, "client-a1");
    client1.write("shared.bin", &payload1());
    let client2 = TestSide::new(&id_b, "client-a2");
    client2.write("shared.bin", &payload2());

    // Run both sessions against the same server root concurrently.
    let (_r1, _r2) = tokio::join!(
        run_client(&client1, addr, &id_b),
        run_client(&client2, addr, &id_b),
    );

    let server_file = server.read("shared.bin");
    assert!(
        server_file == payload1() || server_file == payload2(),
        "the destination must be exactly one whole payload, got {} bytes",
        server_file.len()
    );

    // No temporary file may be stranded anywhere under the server root.
    let leftovers: Vec<_> = std::fs::read_dir(server.dir.path())
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(ferrisync_core::safe_fs::TEMP_PREFIX)
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "no temporary file may be stranded: {leftovers:?}"
    );
}

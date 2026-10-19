//! Milestone 2 acceptance tests: safe filesystem writes.
//!
//! These assert the four exit criteria directly:
//!
//! 1. a failed write leaves the previous destination intact;
//! 2. concurrent transfers to one destination never share a temporary file;
//! 3. a symlink cannot redirect a write outside the synchronization root;
//! 4. a temporary file left behind by a crash is never indexed as a real file.

use ferrisync_core::safe_fs::{OverwritePolicy, SafeFileStore, StoreError, TEMP_PREFIX};
use std::path::Path;

/// A write that fails verification must not touch the existing destination.
#[test]
fn failed_write_leaves_existing_destination_intact() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("keep.txt");
    std::fs::write(&target, b"ORIGINAL").unwrap();

    let store = SafeFileStore::open(dir.path()).unwrap();
    let err = store
        .write_blocking(
            "keep.txt",
            b"NEW-BYTES",
            Some([0u8; 32]),
            OverwritePolicy::Replace,
        )
        .unwrap_err();
    assert!(matches!(err, StoreError::HashMismatch { .. }));
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"ORIGINAL",
        "a rejected write must leave the previous file untouched"
    );
    assert_no_temp_files(dir.path());
}

/// A write whose destination cannot be created must fail cleanly, leaving any
/// unrelated file intact and no temporary file behind.
#[test]
fn failed_write_cleans_up_and_keeps_other_files() {
    let dir = tempfile::tempdir().unwrap();
    // `blocker` is a file, so `blocker/child.txt` can never be created.
    std::fs::write(dir.path().join("blocker"), b"UNRELATED").unwrap();

    let store = SafeFileStore::open(dir.path()).unwrap();
    let err = store
        .write_blocking("blocker/child.txt", b"data", None, OverwritePolicy::Replace)
        .unwrap_err();
    assert!(
        matches!(err, StoreError::UnsafePath { .. } | StoreError::Io { .. }),
        "unexpected error: {err}"
    );
    assert_eq!(
        std::fs::read(dir.path().join("blocker")).unwrap(),
        b"UNRELATED"
    );
    assert_no_temp_files(dir.path());
}

/// Many writers racing for the same destination must each produce a whole,
/// self-consistent file (never a mix), and must never collide on a temp name.
#[test]
fn concurrent_writers_never_share_a_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = SafeFileStore::open(dir.path()).unwrap();

    // Eight distinct 512 KiB payloads, each a single repeated byte.
    let payloads: Vec<Vec<u8>> = (0..8u8).map(|b| vec![b; 512 * 1024]).collect();

    let handles: Vec<_> = payloads
        .iter()
        .cloned()
        .map(|payload| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .write_blocking("shared.bin", &payload, None, OverwritePolicy::Replace)
                    .unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }

    let final_bytes = std::fs::read(dir.path().join("shared.bin")).unwrap();
    assert!(
        payloads.iter().any(|p| p == &final_bytes),
        "the destination must be exactly one writer's payload, never a blend"
    );
    assert_no_temp_files(dir.path());
}

/// A symlinked directory or file must never let a write escape the root.
#[cfg(unix)]
#[test]
fn symlink_cannot_escape_the_sync_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("victim.txt"), b"SAFE").unwrap();

    // A directory symlink pointing outside the root …
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
    // … and a file symlink pointing at a file outside the root.
    std::os::unix::fs::symlink(
        outside.path().join("victim.txt"),
        root.path().join("link.txt"),
    )
    .unwrap();

    let store = SafeFileStore::open(root.path()).unwrap();
    let dir_err = store
        .write_blocking("escape/pwned.txt", b"pwned", None, OverwritePolicy::Replace)
        .unwrap_err();
    assert!(
        matches!(dir_err, StoreError::UnsafePath { .. }),
        "writing through a symlinked directory must be refused: {dir_err}"
    );
    assert!(store
        .write_blocking("link.txt", b"pwned", None, OverwritePolicy::Replace)
        .is_err());

    assert_eq!(
        std::fs::read(outside.path().join("victim.txt")).unwrap(),
        b"SAFE"
    );
    assert!(!outside.path().join("pwned.txt").exists());
}

/// A temporary file left behind by a crash must be invisible to the index, so
/// a restart can never mistake it for a completed transfer.
#[test]
fn leftover_temp_file_is_not_indexed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("real.txt"), b"real").unwrap();
    std::fs::write(
        dir.path().join(format!("{TEMP_PREFIX}-deadbeef")),
        b"partial garbage",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join(TEMP_PREFIX)).unwrap();
    std::fs::write(dir.path().join(TEMP_PREFIX).join("nested"), b"x").unwrap();

    let root = ferrisync_core::filesystem::SyncRoot::open(dir.path().to_path_buf()).unwrap();
    let snapshot = root
        .scan(ferrisync_core::domain::folder::FolderId(1), 0)
        .unwrap();

    let paths: Vec<&str> = snapshot.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"real.txt"), "real files are still indexed");
    assert!(
        paths.iter().all(|p| !p.starts_with(TEMP_PREFIX)),
        "leftover temporary artifacts must never be indexed: {paths:?}"
    );
}

/// Assert the directory (and any subdirectory) holds no in-progress temp file.
fn assert_no_temp_files(dir: &Path) {
    let mut stack = vec![dir.to_path_buf()];
    let mut found = Vec::new();
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).unwrap().filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(TEMP_PREFIX) {
                found.push(entry.path());
            }
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(entry.path());
            }
        }
    }
    assert!(found.is_empty(), "temporary files left behind: {found:?}");
}

//! Security regression suite — filesystem root containment (Phase 3).
//!
//! Objective: every remote-controlled filesystem operation must stay inside the
//! synchronization root even if the attacker can manipulate local directory
//! entries. The critical invariant asserted by every test here is that a
//! *sentinel file outside the root* is unchanged — no operation may create,
//! overwrite, rename, or delete anything outside the root.
//!
//! Adversarial path operations are run against throwaway temp dirs, never real
//! user data.

use ferrisync_core::safe_fs::{OverwritePolicy, SafeFileStore, StoreError, TEMP_PREFIX};
use std::path::{Path, PathBuf};

/// A sync root plus an external sentinel file the tests must never touch.
struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    sentinel: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let sentinel = tmp.path().join("sentinel.txt");
        std::fs::write(&sentinel, b"SENTINEL-DO-NOT-TOUCH").unwrap();
        Self {
            _tmp: tmp,
            root,
            sentinel,
        }
    }

    fn store(&self) -> SafeFileStore {
        SafeFileStore::open(&self.root).unwrap()
    }

    /// Assert the external sentinel still holds exactly its original bytes.
    fn assert_sentinel_intact(&self) {
        assert_eq!(
            std::fs::read(&self.sentinel).unwrap(),
            b"SENTINEL-DO-NOT-TOUCH",
            "an operation escaped the sync root and modified the external sentinel"
        );
    }

    fn assert_no_temp_leftovers(&self) {
        let leftovers = walk(&self.root)
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().starts_with(TEMP_PREFIX))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(leftovers, 0, "temporary files must not be left behind");
    }
}

/// Recursively list every path under `dir` (excluding the dir itself).
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && !p.is_symlink() {
                stack.push(p.clone());
            }
            out.push(p);
        }
    }
    out
}

/// `../` traversal and absolute-path injection are rejected and never write.
#[test]
fn traversal_and_absolute_injection_are_rejected() {
    let fx = Fixture::new();
    let store = fx.store();

    for evil in [
        "../sentinel.txt",
        "../../etc/passwd",
        "/etc/passwd",
        "sub/../../sentinel.txt",
        "foo\0bar",
    ] {
        assert!(
            store
                .write_blocking(evil, b"pwned", None, OverwritePolicy::Replace)
                .is_err(),
            "{evil:?} must be rejected"
        );
    }
    fx.assert_sentinel_intact();
    assert!(fx.sentinel.exists(), "the sentinel must still be present");
    fx.assert_no_temp_leftovers();
}

/// A symlink at the destination (existing or dangling) must be rejected, not
/// followed.
#[cfg(unix)]
#[test]
fn symlink_destination_is_rejected() {
    let fx = Fixture::new();
    let store = fx.store();

    // Existing symlink pointing at the external sentinel.
    std::os::unix::fs::symlink(&fx.sentinel, fx.root.join("link.txt")).unwrap();
    assert!(store
        .write_blocking("link.txt", b"pwned", None, OverwritePolicy::Replace)
        .is_err());

    // Dangling symlink (target does not exist yet).
    std::os::unix::fs::symlink(fx.root.join("nonexistent"), fx.root.join("dangling.txt")).unwrap();
    assert!(store
        .write_blocking("dangling.txt", b"pwned", None, OverwritePolicy::Replace)
        .is_err());

    fx.assert_sentinel_intact();
}

/// A symlink in a *parent* component must be rejected: writing through
/// `sub/link/file.txt` where `link` points outside must not escape.
#[cfg(unix)]
#[test]
fn symlink_parent_component_is_rejected() {
    let fx = Fixture::new();
    let store = fx.store();

    let outside = fx._tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fx.root.join("link")).unwrap();

    assert!(store
        .write_blocking("link/new.txt", b"pwned", None, OverwritePolicy::Replace)
        .is_err());

    assert!(
        !outside.join("new.txt").exists(),
        "a write through a symlinked parent must not land outside the root"
    );
    fx.assert_sentinel_intact();
}

/// Unicode and unusual-but-valid filenames stay inside the root and round-trip.
#[test]
fn unicode_and_unusual_names_stay_contained() {
    let fx = Fixture::new();
    let store = fx.store();

    for name in ["héllo wörld.txt", "emoji-😀.bin", "spaces and (parens).txt"] {
        store
            .write_blocking(name, b"data", None, OverwritePolicy::Replace)
            .unwrap();
        assert_eq!(std::fs::read(fx.root.join(name)).unwrap(), b"data");
    }
    fx.assert_sentinel_intact();
    fx.assert_no_temp_leftovers();
}

/// A directory swapped for a symlink *between* path resolution and the final
/// replacement must not redirect the write. The store re-validates the
/// destination immediately before the rename; this drives that path by making
/// the intermediate directory a symlink to the outside.
#[cfg(unix)]
#[test]
fn directory_to_symlink_swap_cannot_redirect_the_rename() {
    let fx = Fixture::new();
    let store = fx.store();

    // `sub` is a real directory containing a real file to start.
    std::fs::create_dir(fx.root.join("sub")).unwrap();
    std::fs::write(fx.root.join("sub/f.txt"), b"original").unwrap();

    // An attacker replaces `sub` with a symlink to the outside directory.
    let outside = fx._tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("f.txt"), b"attacker").unwrap();
    std::fs::remove_dir_all(fx.root.join("sub")).unwrap();
    std::os::unix::fs::symlink(&outside, fx.root.join("sub")).unwrap();

    // The store must refuse (symlink in path) rather than write through it.
    let result = store.write_blocking("sub/f.txt", b"pwned", None, OverwritePolicy::Replace);
    assert!(
        result.is_err(),
        "a directory replaced by a symlink must not be written through"
    );
    assert_eq!(
        std::fs::read(outside.join("f.txt")).unwrap(),
        b"attacker",
        "the external file must be untouched"
    );
    fx.assert_sentinel_intact();
}

/// A write that fails at the final rename must not leave a temp file that could
/// later be mistaken for a completed transfer.
#[test]
fn failed_finalization_leaves_no_partial_destination() {
    let fx = Fixture::new();
    let store = fx.store();

    // The destination is a directory: the rename onto it must fail.
    std::fs::create_dir(fx.root.join("taken")).unwrap();
    let err = store
        .write_blocking("taken", b"data", None, OverwritePolicy::Replace)
        .unwrap_err();
    assert!(matches!(err, StoreError::Io { .. }), "unexpected: {err}");
    assert!(fx.root.join("taken").is_dir(), "the directory must survive");
    fx.assert_no_temp_leftovers();
    fx.assert_sentinel_intact();
}

/// A conflict backup is created *inside* the root, next to its destination, and
/// never escapes.
#[test]
fn conflict_backup_stays_inside_the_root() {
    let fx = Fixture::new();
    let store = fx.store();

    std::fs::write(fx.root.join("doc.txt"), b"old").unwrap();
    let out = store
        .write_blocking(
            "doc.txt",
            b"new",
            None,
            OverwritePolicy::BackupExisting {
                loser_label: "local".into(),
            },
        )
        .unwrap();
    let backup = out.conflict_backup.expect("backup created");
    assert!(
        backup.starts_with(&fx.root),
        "a conflict backup must live inside the sync root"
    );
    assert_eq!(std::fs::read(&backup).unwrap(), b"old");
    fx.assert_sentinel_intact();
}

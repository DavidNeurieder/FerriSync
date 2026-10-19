//! Milestone 4: property-based tests for path validation and chunk sequencing.
//!
//! These complement the example-based tests: instead of a handful of hand-picked
//! inputs they assert the safety contract over many generated inputs.
//!
//! * Path validation: any path the index validator accepts must be relative,
//!   normalized, NUL-free, and within the length limit — so it can never name a
//!   location outside the synchronization root.
//! * Chunk sequencing: no sequence of chunks may ever surface a file whose
//!   bytes or size disagree with what the session agreed to receive.

use ferrisync_core::protocol::{FileChunk, Index, IndexEntry, MAX_PATH_LEN};
use ferrisync_core::sync_engine::session_transfer::{
    validate_remote_index, ExpectedFile, IncomingTransfers,
};
use proptest::prelude::*;
use std::collections::HashMap;
use std::path::{Component, Path};

fn index_with_paths(paths: Vec<String>) -> Index {
    Index {
        folder_id: "1".into(),
        device_id: "d".into(),
        remote_path: None,
        entries: paths
            .into_iter()
            .map(|path| IndexEntry {
                path,
                local_version: 1,
                remote_version: 0,
                mtime: 0,
                size: 1,
                hash: vec![0u8; 32],
            })
            .collect(),
    }
}

proptest! {
    /// Anything the index validator accepts is a safe relative path.
    #[test]
    fn accepted_index_paths_are_safe(paths in prop::collection::vec(any::<String>(), 0..6)) {
        let index = index_with_paths(paths);
        if validate_remote_index(&index).is_ok() {
            for entry in &index.entries {
                let path = &entry.path;
                prop_assert!(!path.is_empty());
                prop_assert!(path.len() <= MAX_PATH_LEN);
                prop_assert!(!path.contains('\0'));
                let p = Path::new(path);
                prop_assert!(!p.is_absolute());
                for component in p.components() {
                    prop_assert!(
                        matches!(component, Component::Normal(_)),
                        "accepted path {path:?} has a non-normal component"
                    );
                }
            }
        }
    }

    /// A duplicated path can never be accepted, whatever the other entries are.
    #[test]
    fn duplicate_paths_are_always_rejected(
        paths in prop::collection::vec("[A-Za-z0-9_-]{1,12}", 1..5)
    ) {
        let mut all = paths.clone();
        all.push(paths[0].clone());
        let index = index_with_paths(all);
        prop_assert!(validate_remote_index(&index).is_err());
    }

    /// No chunk sequence may surface a file whose size or hash disagrees with the
    /// agreed expectation, and it must never panic.
    #[test]
    fn chunk_sequences_never_surface_invalid_files(
        chunks in prop::collection::vec(
            (
                0usize..3,
                0u64..6,
                prop::collection::vec(any::<u8>(), 0..6),
                0u64..6,
            ),
            0..12,
        ),
    ) {
        // A small agreed set with known bytes: two non-empty files and one empty
        // file, so both success and rejection paths are exercised.
        let agreed: Vec<(&str, Vec<u8>)> = vec![
            ("a", b"AAAA".to_vec()),
            ("b", b"BBBB".to_vec()),
            ("c", Vec::new()),
        ];
        let expected: HashMap<String, ExpectedFile> = agreed
            .iter()
            .map(|(name, data)| {
                (
                    (*name).to_string(),
                    ExpectedFile {
                        size: data.len() as u64,
                        hash: *blake3::hash(data).as_bytes(),
                    },
                )
            })
            .collect();
        let paths: Vec<&str> = agreed.iter().map(|(name, _)| *name).collect();

        let mut transfers = IncomingTransfers::new(expected.clone());
        for (path_idx, offset, data, total_size) in chunks {
            let chunk = FileChunk {
                folder_id: "1".into(),
                path: paths[path_idx].to_string(),
                offset,
                data,
                total_size,
            };
            if let Ok(Some(done)) = transfers.accept(&chunk) {
                let want = expected
                    .get(&done.path)
                    .expect("only agreed paths can complete");
                prop_assert_eq!(done.data.len() as u64, want.size);
                let actual: [u8; 32] = *blake3::hash(&done.data).as_bytes();
                prop_assert_eq!(actual, want.hash);
            }
        }
    }
}

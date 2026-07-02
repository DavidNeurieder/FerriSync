use crate::protocol::{Index, IndexEntry};
use anyhow::Result;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Build a file index by scanning a directory tree.
pub fn build_index(folder_id: String, root: &Path) -> Result<Index> {
    let mut entries = Vec::new();

    if !root.exists() {
        return Ok(Index {
            folder_id,
            entries,
        });
    }

    walk_dir(root, root, &mut entries)?;

    Ok(Index {
        folder_id,
        entries,
    })
}

fn walk_dir(root: &Path, dir: &Path, entries: &mut Vec<IndexEntry>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            walk_dir(root, &path, entries)?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        let metadata = std::fs::metadata(&path)?;
        let mtime = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        let size = metadata.len();

        let hash = compute_blake3(&path)?;

        entries.push(IndexEntry {
            path: relative,
            local_version: mtime as u64,
            remote_version: 0,
            mtime,
            size,
            hash,
        });
    }

    Ok(())
}

fn compute_blake3(path: &Path) -> Result<Vec<u8>> {
    let data = std::fs::read(path)?;
    let hash = blake3::hash(&data);
    Ok(hash.as_bytes().to_vec())
}

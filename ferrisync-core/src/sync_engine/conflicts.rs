//! Conflict inventory and resolution.
//!
//! When a sync session finds a file whose content differs on both sides, the
//! receiving device moves its previous version aside as a
//! `{path}.ferrisync-conflict-{ts}-{loser_label}-{short_hash}` backup before
//! the peer's version is written to the real path (see `backup_on_conflict`
//! in `session.rs`). This module discovers those backups on disk — which makes
//! conflicts visible even after an app restart, unlike the transient, polled
//! events alone — and resolves them without leaking filesystem details to the
//! UI.
//!
//! Resolution actions:
//! - `keep_backup`  — copy the backup contents over the real file, drop the
//!   backup (that version becomes the file).
//! - `keep_original`— drop the backup, keeping the real (winner) file.
//! - `keep_both`    — rename the backup to `{name} (this device){ext}` so both
//!   versions stay on disk but the backup stops being flagged as a conflict.
//!
//! Every action is confined through `SyncRoot` and only ever touches files
//! whose names carry the conflict marker, so this API can never be used to
//! delete or overwrite arbitrary files.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::filesystem::SyncRoot;
use crate::storage::Storage;

/// The marker embedded in every conflict backup file name.
pub const CONFLICT_TAG: &str = ".ferrisync-conflict-";

/// One conflict backup found on disk, with metadata for both versions.
#[derive(Debug, Clone)]
pub struct ConflictEntry {
    /// Sync-folder row id this conflict lives in.
    pub folder_id: i64,
    /// Relative path of the real (winner) file inside the sync folder.
    pub path: String,
    /// Relative path of the conflict backup file inside the sync folder.
    pub backup_path: String,
    /// Which version the backup holds: "local" or "remote" (relative to this
    /// device — in practice the backup always safeguards the previous local
    /// version, so this is "local").
    pub loser_label: String,
    /// Last-modified (unix seconds) of the real file.
    pub winner_mtime_secs: i64,
    /// Size in bytes of the real file.
    pub winner_size: u64,
    /// Last-modified (unix seconds) of the backup file.
    pub loser_mtime_secs: i64,
    /// Size in bytes of the backup file.
    pub loser_size: u64,
}

/// Parse the `{ts}-{loser_label}-{short_hash}` tail of a conflict file name.
fn parse_conflict_suffix(file_name: &str) -> Option<(u64, String, String)> {
    let idx = file_name.rfind(CONFLICT_TAG)?;
    let tail = &file_name[idx + CONFLICT_TAG.len()..];
    let mut parts = tail.splitn(3, '-');
    let ts = parts.next()?.parse().ok()?;
    let label = parts.next()?;
    if label.is_empty() {
        return None;
    }
    let hash = parts.next()?;
    Some((ts, label.to_string(), hash.to_string()))
}

/// The real file's relative path, derived from a conflict backup's relative
/// path by stripping the conflict marker.
fn winner_relative_path(backup_relative: &str) -> Option<String> {
    let file_name = Path::new(backup_relative).file_name()?.to_string_lossy().to_string();
    let idx = file_name.rfind(CONFLICT_TAG)?;
    let original_name = &file_name[..idx];
    let parent = Path::new(backup_relative).parent().map(|p| p.to_string_lossy().to_string());
    Some(match parent {
        Some(dir) if !dir.is_empty() => format!("{dir}/{original_name}"),
        _ => original_name.to_string(),
    })
}

/// Walk a folder tree, collecting every conflict backup while resolving the
/// original file's relative path and both versions' metadata.
fn walk_dir(root: &SyncRoot, dir: &Path, folder_id: i64, out: &mut Vec<ConflictEntry>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let path = entry.path();
        if ft.is_dir() {
            walk_dir(root, &path, folder_id, out)?;
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
            .unwrap_or_default();
        let Some((_ts, _label, _hash)) = parse_conflict_suffix(&file_name) else {
            continue;
        };
        let rel = path
            .strip_prefix(root.root())
            .with_context(|| format!("path escaped sync folder: {}", path.display()))?;
        let backup_relative = rel.to_string_lossy().to_string();
        // A folder can legitimately appear in multiple configured roots; skip
        // duplicates.
        if out.iter().any(|c| c.backup_path == backup_relative) {
            continue;
        }
        let Some(winner_relative) = winner_relative_path(&backup_relative) else {
            continue;
        };
        let backup_meta = std::fs::metadata(&path)?;
        let winner_meta = root.root().join(&winner_relative).metadata().ok();
        out.push(ConflictEntry {
            folder_id,
            path: winner_relative,
            backup_path: backup_relative,
            loser_label: parse_conflict_suffix(&file_name)
                .map(|(_, l, _)| l)
                .unwrap_or_default(),
            winner_mtime_secs: mtime_secs(winner_meta.as_ref()),
            winner_size: winner_meta.map(|m| m.len()).unwrap_or(0),
            loser_mtime_secs: mtime_secs(Some(&backup_meta)),
            loser_size: backup_meta.len(),
        });
    }
    Ok(())
}

/// Unix seconds of a file's modified time, or 0 when unknown.
fn mtime_secs(meta: Option<&std::fs::Metadata>) -> i64 {
    meta.and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// List every conflict backup across all configured sync folders.
///
/// Folders that can no longer be opened are skipped rather than failing the
/// whole listing, so a deleted/moved folder does not hide conflicts elsewhere.
pub fn list_conflicts(storage: &Storage) -> Result<Vec<ConflictEntry>> {
    let mut out = Vec::new();
    for (id, folder_path, _dev, _dir, _last) in storage.list_sync_folders()? {
        let root = match SyncRoot::open(PathBuf::from(folder_path)) {
            Ok(root) => root,
            Err(e) => {
                log::warn!("skipping folder {id} while scanning conflicts: {e:#}");
                continue;
            }
        };
        walk_dir(&root, root.root(), id, &mut out)?;
    }
    Ok(out)
}

/// Resolve a conflict backup using `action` (`keep_backup`, `keep_original`
/// or `keep_both`). Returns the loser label ("local"/"remote") so the caller
/// can describe which version was kept in plain language.
pub async fn resolve_conflict(
    storage: &Storage,
    folder_id: i64,
    backup_path: &str,
    action: &str,
) -> Result<String> {
    let folder = storage
        .list_sync_folders()?
        .into_iter()
        .find(|(id, _, _, _, _)| *id == folder_id)
        .ok_or_else(|| anyhow::anyhow!("sync folder {folder_id} not found"))?;
    let root = SyncRoot::open(PathBuf::from(folder.1))?;

    let file_name = Path::new(backup_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let Some((_ts, label, _hash)) = parse_conflict_suffix(&file_name) else {
        bail!(
            "refusing to resolve {backup_path}: not a ferrisync-conflict backup"
        );
    };
    let winner_relative = winner_relative_path(backup_path)
        .ok_or_else(|| anyhow::anyhow!("invalid conflict path: {backup_path}"))?;

    let backup_abs = root.safe_join(backup_path)?;
    if !backup_abs.exists() {
        bail!("conflict backup missing: {backup_path}");
    }

    match action {
        "keep_backup" => {
            // Overwrite the real file with the backup's version.
            let data = std::fs::read(&backup_abs)
                .with_context(|| format!("read {}", backup_abs.display()))?;
            root.write_file(&winner_relative, &data).await?;
            std::fs::remove_file(&backup_abs)
                .with_context(|| format!("remove {}", backup_abs.display()))?;
            record_resolved(storage, folder_id, &winner_relative, &label, data.len() as i64);
            Ok(label)
        }
        "keep_original" => {
            std::fs::remove_file(&backup_abs)
                .with_context(|| format!("remove {}", backup_abs.display()))?;
            let size = std::fs::metadata(root.root().join(&winner_relative))
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            record_resolved(storage, folder_id, &winner_relative, &label, size);
            Ok(label)
        }
        "keep_both" => {
            // Rename to a plain file so both versions coexist and the backup
            // stops being flagged as a conflict.
            let stem = Path::new(&winner_relative)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());
            let ext = Path::new(&winner_relative)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let suffix = if label == "local" { "this device" } else { "other device" };
            let mut candidate = format!("{stem} ({suffix}){ext}");
            let mut counter = 2u32;
            let parent = match Path::new(&winner_relative).parent() {
                Some(p) if p.as_os_str().is_empty() => String::new(),
                Some(p) => format!("{}/", p.to_string_lossy()),
                None => String::new(),
            };
            while root.root().join(format!("{parent}{candidate}")).exists() {
                candidate = format!("{stem} ({suffix} {counter}){ext}");
                counter += 1;
            }
            let to = format!("{parent}{candidate}");
            let from_param = backup_path;
            let to_param = to;
            root.rename(from_param, &to_param).await?;
            Ok(label)
        }
        other => bail!("unknown conflict action: {other}"),
    }
}

/// Best-effort history row so the activity feed reflects the resolution.
fn record_resolved(storage: &Storage, folder_id: i64, path: &str, label: &str, size: i64) {
    let _ = storage.record_history(crate::storage::HistoryRecord {
        folder_id,
        path,
        device_id: label,
        action: "resolved",
        version: 0,
        mtime: 0,
        hash: &[],
        size,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_conflict_suffix() {
        assert_eq!(
            parse_conflict_suffix("notes.txt.ferrisync-conflict-1700000000-local-abcd1234"),
            Some((1_700_000_000, "local".to_string(), "abcd1234".to_string()))
        );
        assert_eq!(parse_conflict_suffix("plain.txt"), None);
        assert_eq!(parse_conflict_suffix("x.ferrisync-conflict-bad"), None);
    }

    #[test]
    fn derives_winner_path() {
        assert_eq!(
            winner_relative_path("sub/notes.txt.ferrisync-conflict-1-local-h"),
            Some("sub/notes.txt".to_string())
        );
        assert_eq!(
            winner_relative_path("notes.txt.ferrisync-conflict-1-local-h"),
            Some("notes.txt".to_string())
        );
    }
}
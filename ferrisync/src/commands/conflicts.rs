use anyhow::{bail, Context};
use ferrisync_core::sync_engine::conflicts::{list_conflicts, resolve_conflict, ConflictEntry};
use std::collections::HashMap;

use crate::app::ApplicationContext;

/// `ferrisync conflicts [--folder <path>]` — list unresolved conflict backups.
pub fn list(ctx: &ApplicationContext, folder: Option<&str>) -> anyhow::Result<()> {
    let conflicts = list_conflicts(&ctx.storage)?;
    let folder_ids = folder_paths(
        ctx,
        conflicts
            .iter()
            .map(|c| c.folder_id)
            .collect::<Vec<_>>()
            .as_slice(),
    );

    let selected: Vec<ConflictEntry> = match folder {
        Some(name) => {
            let ids: Vec<i64> = ctx
                .storage
                .list_sync_folders()?
                .into_iter()
                .filter(|(_id, path, _dev, _dir, _last)| path == name)
                .map(|(id, _, _, _, _)| id)
                .collect();
            if ids.is_empty() {
                bail!("no sync folder named '{name}'");
            }
            conflicts
                .into_iter()
                .filter(|c| ids.contains(&c.folder_id))
                .collect()
        }
        None => conflicts,
    };

    if selected.is_empty() {
        println!("No unresolved conflicts.");
        return Ok(());
    }

    let mut by_folder: HashMap<i64, Vec<&ConflictEntry>> = HashMap::new();
    for c in &selected {
        by_folder.entry(c.folder_id).or_default().push(c);
    }

    for (folder_id, entries) in by_folder {
        let folder_path = folder_ids.get(&folder_id).cloned().unwrap_or_default();
        println!("{folder_path}");
        for c in &entries {
            let size = crate::commands::fmt::bytes_human(c.winner_size as f64);
            println!(
                "   {path}  ({size}, modified {mtime}) — version on {loser} preserved as a backup",
                path = c.path,
                mtime = crate::commands::fmt::relative(Some(c.winner_mtime_secs)),
                loser = c.loser_label,
            );
        }
    }

    println!("Resolve with: ferrisync conflicts resolve <path> --keep <this|other|both>");
    Ok(())
}

/// `ferrisync conflicts resolve <path> --keep <this|other|both>` — resolve one
/// conflict, keeping one version (or both files).
pub async fn resolve(
    ctx: &ApplicationContext,
    winner_path: &str,
    keep: &str,
) -> anyhow::Result<()> {
    let action = match keep {
        "this" => "keep_original",
        "other" => "keep_backup",
        "both" => "keep_both",
        other => bail!("unknown --keep '{other}' (expected one of: this, other, both)"),
    };

    let conflicts = list_conflicts(&ctx.storage)?;
    let mut matches: Vec<&ConflictEntry> =
        conflicts.iter().filter(|c| c.path == winner_path).collect();

    match matches.len() {
        0 => bail!(
            "no conflict for '{winner_path}' — run `ferrisync conflicts` to list them"
        ),
        1 => {
            // Clone so we can drop the borrow and resolve by backup path.
            let c = matches.remove(0);
            let (folder_id, backup_path, path) = (c.folder_id, c.backup_path.clone(), c.path.clone());
            let loser = resolve_conflict(&ctx.storage, folder_id, &backup_path, action)
                .await
                .with_context(|| format!("resolve conflict {path}"))?;
            match keep {
                "this" => println!("Kept the current version of {path} and removed the backup."),
                "other" => println!(
                    "Replaced {path} with the preserved version (kept the other side)."
                ),
                _ => println!("Kept both versions of {path} (the backup is now a separate file)."),
            }
            let _ = loser;
            Ok(())
        }
        many => bail!(
            "conflict for '{winner_path}' appears in multiple folders ({many}); resolve them one at a time by folder"
        ),
    }
}

/// Map conflict `folder_id`s to their configured local path.
fn folder_paths(ctx: &ApplicationContext, ids: &[i64]) -> HashMap<i64, String> {
    ctx.storage
        .list_sync_folders()
        .map(|folders| {
            folders
                .into_iter()
                .filter(|(id, _, _, _, _)| ids.contains(id))
                .map(|(id, path, _, _, _)| (id, path))
                .collect()
        })
        .unwrap_or_default()
}

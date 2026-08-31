use serde::Serialize;

use crate::app::ApplicationContext;

use super::fmt;

/// `ferrisync activity` — recent sync sessions plus file-level changes.
pub fn run(ctx: &ApplicationContext, limit: u32, json: bool) -> anyhow::Result<()> {
    let sessions = ctx.storage.list_recent_sessions(limit)?;
    let history = ctx.storage.list_file_history(None, limit)?;

    if json {
        let report = ActivityReport {
            sessions,
            file_events: history,
        };
        let out = serde_json::to_string_pretty(&report)?;
        println!("{out}");
        return Ok(());
    }

    if sessions.is_empty() && history.is_empty() {
        println!("No activity recorded yet.");
        return Ok(());
    }

    if !sessions.is_empty() {
        println!("Sessions (newest first):");
        for s in &sessions {
            let when = fmt::relative(Some(s.ts));
            let total = s.pushed_count + s.pulled_count;
            let bytes = s.pushed_bytes + s.pulled_bytes;
            println!(
                "  {when}: {dir} sync with {} — {total} file(s), {} ({}, +{}⟶ /{}⟵), conflicts {}",
                s.peer_device,
                fmt::bytes_human(bytes as f64),
                s.folder_path,
                s.pushed_count,
                s.pulled_count,
                s.conflicts_count,
                dir = s.direction,
            );
        }
    }

    if !history.is_empty() {
        println!("\nFile changes (newest first):");
        let device_names = device_names(ctx);
        for h in &history {
            let who = h
                .device_id
                .as_deref()
                .and_then(|d| device_names.get(d))
                .map(|s| s.as_str())
                .unwrap_or("-");
            println!(
                "  {when}: {action:<7} {} ({}) [via {who}]",
                h.path,
                h.size
                    .map(|s| fmt::bytes_human(s as f64))
                    .unwrap_or_else(|| "-".into()),
                when = fmt::relative(Some(h.recorded_at)),
                action = h.action,
            );
        }
    }
    Ok(())
}

/// Display names for file-history device ids (devices may be renamed/removed
/// after a change was recorded).
fn device_names(ctx: &ApplicationContext) -> std::collections::HashMap<String, String> {
    ctx.storage
        .list_devices()
        .map(|devs| devs.into_iter().map(|(id, name, _)| (id, name)).collect())
        .unwrap_or_default()
}

#[derive(Serialize)]
struct ActivityReport {
    sessions: Vec<ferrisync_core::storage::SessionRecord>,
    file_events: Vec<ferrisync_core::storage::FileHistoryRow>,
}

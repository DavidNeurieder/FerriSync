use serde::Serialize;

use crate::app::ApplicationContext;

use ferrisync_core::health::{self, FolderStatus, HealthSnapshot, Presence};

use super::fmt;

/// Snapshot of pairing + folder health for display.
pub struct Status {
    pub snapshot: HealthSnapshot,
    pub device_id: String,
    pub device_name: String,
    /// Remote-path label per folder id, for `local_path ↔ remote_path`.
    pub remote_labels: std::collections::HashMap<i64, String>,
}

pub fn run(ctx: &ApplicationContext) -> anyhow::Result<Status> {
    let snapshot = health::snapshot(
        &ctx.storage,
        &ctx.device_info.id,
        health::now_secs(),
        &health::LiveState::default(),
    )?;
    Ok(Status {
        remote_labels: ctx.storage.folder_remote_labels(&ctx.device_info.id)?,
        snapshot,
        device_id: ctx.device_info.id.clone(),
        device_name: ctx.device_info.name.clone(),
    })
}

/// Default human "how is everything?" output (also what the REPL prints).
pub fn format(status: &Status) -> String {
    format_human(status, false)
}

/// `--verbose` variant: adds device ids and absolute timestamps.
pub fn format_human(status: &Status, verbose: bool) -> String {
    let mut out = String::new();
    let s = &status.snapshot;
    // Show one row per physical folder, not one per (folder, device) pair.
    let folders = health::group_folders(&s.folders, &status.device_id);

    out.push_str(&format!("FerriSync · {}\n", status.device_name));
    out.push_str(&headline(&s.summary));
    out.push('\n');

    out.push_str("DEVICES\n");
    if s.devices.is_empty() {
        out.push_str("  (none — connect a device)\n");
    }
    for d in &s.devices {
        let (dot, tag) = presence_tag(d.presence);
        if verbose {
            out.push_str(&format!("  {dot} {:<24}{}  ({})\n", d.name, tag, d.id));
        } else {
            out.push_str(&format!("  {dot} {:<24}{}\n", d.name, tag));
        }
    }

    out.push_str("FOLDERS\n");
    if folders.is_empty() {
        out.push_str("  (none — add a folder to sync)\n");
    }
    for f in &folders {
        // Show the remote *folder path* on the peer, not just the peer name.
        let peer = status
            .remote_labels
            .get(&f.id)
            .cloned()
            .unwrap_or_else(|| f.peer_label(&status.device_id));
        let conflicts = if f.conflicts > 0 {
            format!(", {} conflict{}", f.conflicts, plural(f.conflicts))
        } else {
            String::new()
        };
        if verbose {
            out.push_str(&format!(
                "  [{id}] {path} ↔ {peer} ({dev}) — {health}, last sync: {iso} ({rel}), {conflicts} conflict(s)\n",
                id = f.id,
                path = f.path,
                dev = f.device_id,
                health = folder_health_label(f.health),
                iso = fmt::iso(f.last_sync),
                rel = fmt::relative(f.last_sync),
                conflicts = f.conflicts,
            ));
        } else {
            out.push_str(&format!(
                "  {path} ↔ {peer} — {health}, last sync: {rel}{conflicts}\n",
                path = f.path,
                peer = peer,
                health = folder_health_label(f.health),
                rel = fmt::relative(f.last_sync),
                conflicts = conflicts,
            ));
        }
    }

    let attention: Vec<&FolderStatus> = folders
        .iter()
        .filter(|f| f.health.needs_attention())
        .collect();
    if !attention.is_empty() {
        out.push_str("\nATTENTION\n");
        for f in &attention {
            out.push_str(&format!(
                "  {path} — {reason}\n",
                path = f.path,
                reason = attention_reason(f, &status.device_id)
            ));
        }
    }

    out.push('\n');
    if verbose {
        out.push_str(&format!("Device ID: {}\n", status.device_id));
    }
    match s.summary.last_sync_secs {
        Some(secs) => out.push_str(&format!("Last sync: {}\n", fmt::relative(Some(secs)))),
        None => out.push_str("Last sync: never\n"),
    }
    out
}

/// Machine-readable `status --json`.
pub fn format_json(status: &Status) -> String {
    let report = StatusReport {
        device: DeviceReport {
            id: status.device_id.clone(),
            name: status.device_name.clone(),
        },
        health: status.snapshot.summary.clone(),
        devices: status.snapshot.devices.clone(),
        folders: status.snapshot.folders.clone(),
    };
    serde_json::to_string_pretty(&report).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

/// One-line "how is everything?" headline derived from the roll-up. Shared by
/// the CLI `status` header and the REPL startup dashboard so both always say
/// the same thing about the system.
pub fn headline(summary: &health::HealthSummary) -> String {
    if summary.folders_total == 0 && summary.device_total == 0 {
        "Start by connecting a device.".to_string()
    } else if summary.conflicts_total > 0 {
        format!(
            "⚠ {} conflict{} need attention",
            summary.conflicts_total,
            plural(summary.conflicts_total)
        )
    } else if summary.folders_needing_attention > 0 {
        format!(
            "↑ {} folder{} need attention",
            summary.folders_needing_attention,
            plural(summary.folders_needing_attention)
        )
    } else {
        let offline = summary
            .device_total
            .saturating_sub(summary.device_connected + summary.device_recently_seen);
        if offline > 0 {
            format!("○ {} device{} offline", offline, plural(offline))
        } else {
            "✓ Everything is synced".to_string()
        }
    }
}

/// Compact folder-centric startup view: headline, then devices and folders.
/// Full detail remains available via `status` / `folders`.
pub fn dashboard(status: &Status) -> String {
    let mut out = String::new();
    let s = &status.snapshot;
    let folders = health::group_folders(&s.folders, &status.device_id);
    out.push_str(&format!("FerriSync · {}\n", status.device_name));
    out.push_str(&headline(&s.summary));
    out.push('\n');

    out.push_str("Devices\n");
    if s.devices.is_empty() {
        out.push_str("  (none — run `pair <ip>` or `discover` to connect)\n");
    }
    for d in &s.devices {
        let (dot, tag) = presence_tag(d.presence);
        out.push_str(&format!("  {dot} {}{tag}\n", d.name));
    }

    out.push_str("Folders\n");
    if folders.is_empty() {
        out.push_str("  (none — add a folder to sync)\n");
    }
    for f in &folders {
        let marker = match f.health {
            health::FolderHealth::Healthy => "✓",
            health::FolderHealth::Syncing => "↑",
            health::FolderHealth::Conflict => "⚠",
            _ => "•",
        };
        let peer = status
            .remote_labels
            .get(&f.id)
            .cloned()
            .unwrap_or_else(|| f.peer_label(&status.device_id));
        out.push_str(&format!(
            "  {marker} {path} ↔ {peer} — {health}\n",
            path = f.path,
            peer = peer,
            health = folder_health_label(f.health),
        ));
    }
    out
}

#[derive(Serialize)]
struct StatusReport {
    device: DeviceReport,
    health: health::HealthSummary,
    devices: Vec<health::DeviceStatus>,
    folders: Vec<health::FolderStatus>,
}

#[derive(Serialize)]
struct DeviceReport {
    id: String,
    name: String,
}

/// Dot + presence tag for the compact device rows in the dashboard.
fn presence_tag(presence: Presence) -> (&'static str, String) {
    match presence {
        Presence::Connected => ("●", " · connected".to_string()),
        Presence::RecentlySeen => ("●", " · recently seen".to_string()),
        Presence::Offline => ("○", " · offline".to_string()),
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn folder_health_label(h: health::FolderHealth) -> &'static str {
    match h {
        health::FolderHealth::Healthy => "healthy",
        health::FolderHealth::Syncing => "syncing",
        health::FolderHealth::Waiting => "waiting",
        health::FolderHealth::Offline => "offline",
        health::FolderHealth::Error => "error",
        health::FolderHealth::Conflict => "conflict",
        health::FolderHealth::NotConfigured => "not configured",
    }
}

fn attention_reason(f: &FolderStatus, own_device_id: &str) -> String {
    let peer = f.peer_label(own_device_id);
    match f.health {
        health::FolderHealth::Conflict => format!(
            "{} unresolved conflict(s), resolve with: ferrisync conflicts",
            f.conflicts
        ),
        health::FolderHealth::Offline => {
            format!("{peer} is offline and {} has never synced", f.path)
        }
        health::FolderHealth::Error => "last sync errored — run `ferrisync sync` again".into(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrisync_core::health::{DeviceStatus, FolderHealth, FolderStatus, HealthSnapshot};

    fn folder(
        path: &str,
        device_id: &str,
        device_name: Option<&str>,
        health: FolderHealth,
        conflicts: usize,
        last_sync: Option<i64>,
    ) -> FolderStatus {
        FolderStatus {
            id: 0,
            path: path.into(),
            device_id: device_id.into(),
            device_name: device_name.map(|s| s.into()),
            health,
            last_sync,
            conflicts,
        }
    }

    fn device(name: &str, id: &str, presence: Presence, folder_count: usize) -> DeviceStatus {
        DeviceStatus {
            id: id.into(),
            name: name.into(),
            presence,
            folder_count,
        }
    }

    fn status(devices: Vec<DeviceStatus>, folders: Vec<FolderStatus>) -> Status {
        let snapshot = HealthSnapshot {
            summary: health::summarize(&devices, &folders),
            devices,
            folders,
        };
        Status {
            snapshot,
            device_id: "self-uuid".into(),
            device_name: "Desk".into(),
            remote_labels: Default::default(),
        }
    }

    fn healthy_status() -> Status {
        status(
            vec![device("Pixel 9", "phone", Presence::Connected, 1)],
            vec![folder(
                "~/Photos",
                "phone",
                Some("Pixel 9"),
                FolderHealth::Healthy,
                0,
                Some(1000),
            )],
        )
    }

    fn empty_status() -> Status {
        status(vec![], vec![])
    }

    #[test]
    fn default_output_hides_internal_ids() {
        let out = format(&healthy_status());
        assert!(out.contains("Pixel 9"), "missing device name:\n{out}");
        assert!(out.contains("connected"), "missing presence:\n{out}");
        assert!(!out.contains("phone"), "leaked device id:\n{out}");
        assert!(!out.contains("self-uuid"), "leaked own id:\n{out}");
    }

    #[test]
    fn verbose_output_shows_ids() {
        let out = format_human(&healthy_status(), true);
        assert!(out.contains("phone"), "verbose should show id:\n{out}");
        assert!(out.contains("self-uuid"), "\n{out}");
    }

    #[test]
    fn healthy_folders_render_as_synced() {
        let out = format(&healthy_status());
        assert!(out.contains("healthy"), "expected healthy:\n{out}");
        assert!(!out.contains("ATTENTION"), "no attention expected:\n{out}");
    }

    #[test]
    fn offline_device_renders_with_offline_folders() {
        let s = status(
            vec![device("Laptop", "laptop", Presence::Offline, 1)],
            vec![folder(
                "~/Docs",
                "laptop",
                Some("Laptop"),
                FolderHealth::Offline,
                0,
                None,
            )],
        );
        let out = format(&s);
        assert!(out.contains("offline"), "expected offline label:\n{out}");
        assert!(
            out.contains("ATTENTION"),
            "expected attention section:\n{out}"
        );
        assert!(out.contains("Laptop is offline"), "\n{out}");
    }

    #[test]
    fn syncing_folder_renders_syncing() {
        let s = status(
            vec![device("Pixel 9", "phone", Presence::Connected, 1)],
            vec![folder(
                "~/Photos",
                "phone",
                Some("Pixel 9"),
                FolderHealth::Syncing,
                0,
                None,
            )],
        );
        let out = format(&s);
        assert!(out.contains("syncing"), "expected syncing label:\n{out}");
    }

    #[test]
    fn conflict_folder_renders_and_lists_attention() {
        let s = status(
            vec![device("Pixel 9", "phone", Presence::Connected, 1)],
            vec![folder(
                "~/Photos",
                "phone",
                Some("Pixel 9"),
                FolderHealth::Conflict,
                2,
                Some(1000),
            )],
        );
        let out = format(&s);
        assert!(out.contains("conflict"), "expected conflict label:\n{out}");
        assert!(out.contains("2 unresolved conflict(s)"), "\n{out}");
    }

    #[test]
    fn errored_folder_renders_error() {
        let s = status(
            vec![device("Pixel 9", "phone", Presence::Connected, 1)],
            vec![folder(
                "~/Photos",
                "phone",
                Some("Pixel 9"),
                FolderHealth::Error,
                0,
                Some(1000),
            )],
        );
        let out = format(&s);
        assert!(out.contains("error"), "expected error label:\n{out}");
        assert!(out.contains("run `ferrisync sync` again"), "\n{out}");
    }

    #[test]
    fn no_devices_and_no_folders_render_gracefully() {
        let out = format(&empty_status());
        assert!(out.contains("(none"), "expected (none) placeholder:\n{out}");
    }

    #[test]
    fn json_is_stable_and_parseable() {
        let out = format_json(&healthy_status());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["device"]["name"], "Desk", "{out}");
        for key in ["health", "devices", "folders"] {
            assert!(v.get(key).is_some(), "missing {key} in:\n{out}");
        }
        assert_eq!(v["folders"][0]["health"], "healthy", "{out}");
    }

    #[test]
    fn headline_all_clear_when_healthy() {
        let s = healthy_status();
        assert_eq!(headline(&s.snapshot.summary), "✓ Everything is synced");
    }

    #[test]
    fn headline_flags_conflicts() {
        let s = status(
            vec![device("Pixel 9", "phone", Presence::Connected, 1)],
            vec![folder(
                "~/Photos",
                "phone",
                Some("Pixel 9"),
                FolderHealth::Healthy,
                2,
                Some(1000),
            )],
        );
        assert_eq!(
            headline(&s.snapshot.summary),
            "⚠ 2 conflicts need attention"
        );
    }

    #[test]
    fn headline_flags_folders_needing_attention() {
        let s = status(
            vec![device("Laptop", "laptop", Presence::Offline, 1)],
            vec![folder(
                "~/Docs",
                "laptop",
                Some("Laptop"),
                FolderHealth::Offline,
                0,
                None,
            )],
        );
        assert_eq!(headline(&s.snapshot.summary), "↑ 1 folder need attention");
    }

    #[test]
    fn headline_reports_offline_devices() {
        let s = status(
            vec![
                device("Pixel 9", "phone", Presence::Connected, 1),
                device("Laptop", "laptop", Presence::Offline, 0),
            ],
            vec![folder(
                "~/Photos",
                "phone",
                Some("Pixel 9"),
                FolderHealth::Healthy,
                0,
                Some(1000),
            )],
        );
        assert_eq!(headline(&s.snapshot.summary), "○ 1 device offline");
    }

    #[test]
    fn headline_encourages_connect_when_empty() {
        assert_eq!(
            headline(&empty_status().snapshot.summary),
            "Start by connecting a device."
        );
    }
}

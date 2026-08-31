use serde::Serialize;

use crate::app::ApplicationContext;

use ferrisync_core::health::{self, FolderStatus, HealthSnapshot, Presence};

use super::fmt;

/// Snapshot of pairing + folder health for display.
pub struct Status {
    pub snapshot: HealthSnapshot,
    pub device_id: String,
    pub device_name: String,
}

pub fn run(ctx: &ApplicationContext) -> anyhow::Result<Status> {
    let snapshot = health::snapshot(
        &ctx.storage,
        &ctx.device_info.id,
        health::now_secs(),
        &health::LiveState::default(),
    )?;
    Ok(Status {
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

    out.push_str("Paired devices:\n");
    if s.devices.is_empty() {
        out.push_str("  (none)\n");
    }
    for d in &s.devices {
        let last = presence_label(d.presence).to_string();
        let folders = if d.folder_count == 1 {
            "1 folder".to_string()
        } else {
            format!("{} folders", d.folder_count)
        };
        if verbose {
            out.push_str(&format!(
                "  {:<24} {:<14} {}  ({})\n",
                d.name, last, folders, d.id
            ));
        } else {
            out.push_str(&format!("  {:<24} {:<14} {}\n", d.name, last, folders));
        }
    }
    let connected = s.summary.device_connected;
    let recently = s.summary.device_recently_seen;
    if !s.devices.is_empty() {
        out.push_str(&format!(
            "  → {connected} connected, {recently} recently seen, {} offline\n",
            s.devices.len() - connected - recently
        ));
    }

    out.push_str("\nSync folders:\n");
    if s.folders.is_empty() {
        out.push_str("  (none)\n");
    }
    for f in &s.folders {
        let peer = f.peer_label(&status.device_id);
        if verbose {
            out.push_str(&format!(
                "  [{id}] {path} ↔ {peer} ({dev}) — {health}, last sync: {last} ({rel}), {conflicts} conflict(s)\n",
                id = f.id,
                path = f.path,
                dev = f.device_id,
                health = folder_health_label(f.health),
                last = fmt::iso(f.last_sync),
                rel = fmt::relative(f.last_sync),
                conflicts = f.conflicts,
            ));
        } else {
            out.push_str(&format!(
                "  {path} ↔ {peer} — {health}, last sync: {rel}, {conflicts} conflict(s)\n",
                path = f.path,
                peer = peer,
                health = folder_health_label(f.health),
                rel = fmt::relative(f.last_sync),
                conflicts = f.conflicts,
            ));
        }
    }

    let attention: Vec<&FolderStatus> = s
        .folders
        .iter()
        .filter(|f| f.health.needs_attention())
        .collect();
    if !attention.is_empty() {
        out.push_str("\nATTENTION\n");
        for f in &attention {
            out.push_str(&format!(
                "  {} — {}\n",
                f.path,
                attention_reason(f, &status.device_id)
            ));
        }
    }

    out.push('\n');
    if verbose {
        out.push_str(&format!("Device ID: {}\n", status.device_id));
    }
    out.push_str(&format!("Device name: {}\n", status.device_name));
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

fn presence_label(presence: Presence) -> &'static str {
    match presence {
        Presence::Connected => "connected",
        Presence::RecentlySeen => "recently seen",
        Presence::Offline => "offline",
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
        assert!(
            out.contains("(none)"),
            "expected (none) placeholder:\n{out}"
        );
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
}

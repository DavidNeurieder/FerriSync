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

    let attention: Vec<&FolderStatus> =
        s.folders.iter().filter(|f| f.health.needs_attention()).collect();
    if !attention.is_empty() {
        out.push_str("\nATTENTION\n");
        for f in &attention {
            out.push_str(&format!("  {} — {}\n", f.path, attention_reason(f, &status.device_id)));
        }
    }

    out.push('\n');
    out.push_str(&format!("Device ID: {}\n", status.device_id));
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
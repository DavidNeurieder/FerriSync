use crate::app::ApplicationContext;

/// Snapshot of pairing + folder configuration for display.
pub struct Status {
    pub devices: Vec<(String, String, Option<i64>)>,
    pub folders: Vec<(i64, String, String, String, Option<i64>)>,
    pub device_id: String,
    pub device_name: String,
}

pub fn run(ctx: &ApplicationContext) -> anyhow::Result<Status> {
    Ok(Status {
        devices: ctx.storage.list_devices()?,
        folders: ctx.storage.list_sync_folders()?,
        device_id: ctx.device_info.id.clone(),
        device_name: ctx.device_info.name.clone(),
    })
}

pub fn format(status: &Status) -> String {
    let mut out = String::new();
    out.push_str("Paired devices:\n");
    if status.devices.is_empty() {
        out.push_str("  (none)\n");
    }
    for (id, name, last_seen) in &status.devices {
        let last = last_seen.map_or("never".to_string(), |ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| ts.to_string())
        });
        out.push_str(&format!("  {name} ({id}) — last seen: {last}\n"));
    }

    out.push_str("Sync folders:\n");
    if status.folders.is_empty() {
        out.push_str("  (none)\n");
    }
    for (id, path, dev_id, dir, last_sync) in &status.folders {
        let last = last_sync.map_or("never".to_string(), |ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| ts.to_string())
        });
        out.push_str(&format!("  [{id}] {path} ↔ {dev_id} ({dir}) — last sync: {last}\n"));
    }

    out.push('\n');
    out.push_str(&format!("Device ID: {}\n", status.device_id));
    out.push_str(&format!("Device name: {}\n", status.device_name));
    out
}
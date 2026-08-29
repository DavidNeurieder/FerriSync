use ferrisync_core::storage::Storage;
use ferrisync_core::DeviceInfo;
use std::sync::Arc;

pub async fn run(storage: Arc<Storage>, device_info: DeviceInfo) -> anyhow::Result<()> {
    let devices = storage.list_devices()?;
    println!("Paired devices:");
    if devices.is_empty() {
        println!("  (none)");
    }
    for (id, name, last_seen) in &devices {
        let last = last_seen.map_or("never".to_string(), |ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| ts.to_string())
        });
        println!("  {name} ({id}) — last seen: {last}");
    }

    let folders = storage.list_sync_folders()?;
    println!("Sync folders:");
    if folders.is_empty() {
        println!("  (none)");
    }
    for (id, path, dev_id, dir, last_sync) in &folders {
        let last = last_sync.map_or("never".to_string(), |ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| ts.to_string())
        });
        println!("  [{id}] {path} ↔ {dev_id} ({dir}) — last sync: {last}");
    }

    println!();
    println!("Device ID: {}", device_info.id);
    println!("Device name: {}", device_info.name);
    Ok(())
}

use anyhow::Result;
use ferrisync_core::storage::Storage;
use std::sync::Arc;

use super::read_yes_no;

pub async fn run(device_id: &str, yes: bool, storage: &Arc<Storage>) -> Result<()> {
    // Look up device name before deletion for the prompt.
    let device_name = storage
        .list_devices()?
        .into_iter()
        .find(|(id, _, _)| id == device_id)
        .map(|(_, name, _)| name);

    match &device_name {
        Some(name) => {
            if !yes {
                println!(
                    "Remove device '{name}' ({device_id})? \
                     This deletes all associated folders and history."
                );
                print!("Continue? [y/N] ");
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
                if !read_yes_no().await {
                    println!("Aborted.");
                    return Ok(());
                }
            }
        }
        None => {
            anyhow::bail!("Device {device_id} not found.");
        }
    }

    let c = storage.remove_device(device_id)?;
    println!("Removed device '{}'.", device_name.unwrap_or_default());
    if c.folders_removed > 0 {
        println!("  {} folder(s) deleted", c.folders_removed);
    }
    if c.sessions_removed > 0 {
        println!("  {} session(s) deleted", c.sessions_removed);
    }
    if c.history_removed > 0 {
        println!("  {} history entry/entries deleted", c.history_removed);
    }
    if c.metadata_removed > 0 {
        println!("  {} file metadata entry/entries deleted", c.metadata_removed);
    }
    Ok(())
}
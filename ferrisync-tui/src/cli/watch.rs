use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::SyncEngine;
use ferrisync_core::watcher::FileWatcher;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn run(
    folder: String,
    storage: Arc<Storage>,
    _engine: Arc<SyncEngine>,
) -> anyhow::Result<()> {
    println!("Watching {folder} for changes... (press Ctrl+C to stop)");

    let (tx, mut rx) = tokio::sync::mpsc::channel(256);

    let watch_folder = folder.clone();
    tokio::spawn(async move {
        let mut watcher = match FileWatcher::watch(PathBuf::from(&watch_folder)) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to watch {watch_folder}: {e}");
                return;
            }
        };
        while let Some(event) = watcher.events().recv().await {
            let _ = tx.send(event).await;
        }
    });

    while let Some(event) = rx.recv().await {
        println!("Change detected: {event:?}");
        let devices = storage.list_devices()?;
        for (dev_id, name, _) in &devices {
            println!("  → would sync with {name} ({dev_id})");
        }
    }

    Ok(())
}

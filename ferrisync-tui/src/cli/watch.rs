use ferrisync_core::crypto::CryptoProvider;
use ferrisync_core::storage::Storage;
use ferrisync_core::sync_engine::session;
use ferrisync_core::watcher::FileWatcher;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn run(
    folder: String,
    device: String,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
) -> anyhow::Result<()> {
    let folder_id = storage.add_sync_folder(&folder, &device, "bidirectional")?;
    let remote_addr: std::net::SocketAddr = device.parse()?;

    println!("Watching {folder} for changes, syncing to {remote_addr}... (press Ctrl+C to stop)");

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
        while let Some(_event) = watcher.events().recv().await {
            let _ = tx.send(()).await;
        }
    });

    while rx.recv().await.is_some() {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        while rx.try_recv().is_ok() {}
        println!("Change detected, syncing...");
        let (event_tx, _) = tokio::sync::mpsc::channel(256);
        match session::run_sync_session(
            crypto.clone(),
            storage.clone(),
            &folder,
            remote_addr,
            folder_id,
            "",
            event_tx,
        ).await {
            Ok(result) => {
                println!("Sync complete. Pushed: {}, Pulled: {}, Conflicts: {}",
                    result.pushed.len(),
                    result.pulled.len(),
                    result.conflicts.len(),
                );
            }
            Err(e) => {
                eprintln!("Sync failed: {e}");
            }
        }
    }

    Ok(())
}

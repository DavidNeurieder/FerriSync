use ferrisync_core::persistence::InMemoryStateStore;
use ferrisync_core::storage::Storage;
use ferrisync_core::{CryptoProvider, DeviceInfo, SyncEngine, FileWatcher};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;

use super::parse_device;

/// One-shot: watch a folder and re-sync on every change until killed.
pub async fn run(
    folder: String,
    device: String,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
) -> anyhow::Result<()> {
    let addr = parse_device(&device, super::DEFAULT_PORT)?;
    let folder_id = get_or_create_folder(&storage, &folder, &device)?;
    println!("Watching {folder}, syncing with {addr}... (press Ctrl+C to stop)");

    let (_shutdown_tx, shutdown_rx) = watch::channel(false);
    watch_loop(folder, addr, folder_id, storage, crypto, shutdown_rx).await
}

/// Reuse an existing sync-folder row for (path, device) if one exists.
pub fn get_or_create_folder(storage: &Storage, path: &str, device: &str) -> anyhow::Result<i64> {
    for (id, p, dev, _dir, _last_sync) in storage.list_sync_folders()? {
        if p == path && dev == device {
            return Ok(id);
        }
    }
    super::ensure_device(storage, device)?;
    storage.add_sync_folder(path, device, "bidirectional")
}

/// Sync-on-change loop. Exits when the shutdown channel fires (value change
/// or sender drop). Runs either in the foreground (one-shot CLI) or inside a
/// spawned task (REPL background watches).
pub async fn watch_loop(
    folder: String,
    remote_addr: SocketAddr,
    folder_id: i64,
    storage: Arc<Storage>,
    crypto: Arc<CryptoProvider>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut watcher = FileWatcher::watch(PathBuf::from(&folder))
        .map_err(|e| anyhow::anyhow!("Failed to watch {folder}: {e}"))?;

    let state_store = Arc::new(InMemoryStateStore::new());
    let engine = SyncEngine::new(
        storage,
        crypto,
        DeviceInfo {
            id: String::new(),
            name: String::new(),
            cert_fingerprint: Vec::new(),
        },
        state_store,
    );

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            event = watcher.events().recv() => {
                if event.is_none() {
                    break;
                }
                // Debounce: wait for the burst to settle, then drain queued events.
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                while watcher.events().try_recv().is_ok() {}

                println!("[watch:{folder}] change detected, syncing...");
                match engine.run_sync(&folder, remote_addr, folder_id, "").await {
                    Ok(result) => println!(
                        "[watch:{folder}] pushed {}, pulled {}, conflicts {}",
                        result.pushed.len(),
                        result.pulled.len(),
                        result.conflicts.len(),
                    ),
                    Err(e) => eprintln!("[watch:{folder}] sync failed: {e}"),
                }
            }
        }
    }

    println!("[watch:{folder}] stopped");
    Ok(())
}

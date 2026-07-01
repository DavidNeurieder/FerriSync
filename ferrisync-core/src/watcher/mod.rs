use anyhow::Result;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::mpsc;
use tokio::sync::mpsc as tokio_mpsc;

/// File system change events from the watcher.
#[derive(Debug, Clone)]
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed(PathBuf, PathBuf),
}

/// Watches a directory tree for file changes.
pub struct FileWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    rx: tokio_mpsc::Receiver<FileEvent>,
}

impl FileWatcher {
    /// Start watching the given directory. Sends events to the returned receiver.
    pub fn watch(path: PathBuf) -> Result<Self> {
        let (tx_raw, rx_raw) = mpsc::channel();
        let (tx, rx) = tokio_mpsc::channel(256);

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx_raw.send(event);
                }
            },
            Config::default(),
        )?;

        watcher.watch(&path, RecursiveMode::Recursive)?;

        // Bridge from blocking mpsc to tokio mpsc
        tokio::task::spawn_blocking(move || {
            while let Ok(event) = rx_raw.recv() {
                match event.kind {
                    EventKind::Create(_) => {
                        for p in event.paths {
                            let _ = tx.blocking_send(FileEvent::Created(p));
                        }
                    }
                    EventKind::Modify(_) => {
                        for p in event.paths {
                            let _ = tx.blocking_send(FileEvent::Modified(p));
                        }
                    }
                    EventKind::Remove(_) => {
                        for p in event.paths {
                            let _ = tx.blocking_send(FileEvent::Deleted(p));
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(Self { watcher, rx })
    }

    /// Returns a reference to the event receiver.
    pub fn events(&mut self) -> &mut tokio_mpsc::Receiver<FileEvent> {
        &mut self.rx
    }
}

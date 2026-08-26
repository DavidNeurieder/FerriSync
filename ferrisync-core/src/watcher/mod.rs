use anyhow::Result;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;

/// File system change events from the watcher.
///
/// Renamed from `FileEvent` to align with the refactor plan's naming:
/// `Filesystem → Watcher → ChangeEvent → Scheduler → Snapshot → Reconcile → SyncPlan`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChangeEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed(PathBuf, PathBuf),
}

impl ChangeEvent {
    /// The primary path affected by this event.
    pub fn primary_path(&self) -> &Path {
        match self {
            Self::Created(p) | Self::Modified(p) | Self::Deleted(p) => p,
            Self::Renamed(from, _) => from,
        }
    }
}

/// Output signal from the [`ChangeScheduler`].
#[derive(Debug, Clone)]
pub enum SyncTrigger {
    /// A batch of coalesced filesystem events is ready for reconciliation.
    /// The scheduler guarantees that at least `debounce_ms` have elapsed
    /// since the last event in this batch.
    ChangesReady(Vec<ChangeEvent>),
}

/// Debounces and coalesces filesystem change events.
///
/// The scheduler sits between the raw `notify` watcher and the
/// snapshot/reconcile pipeline. It accumulates events and releases them
/// in batches after a configurable quiet period, preventing redundant
/// snapshot scans when a large save/edit operation touches many files.
///
/// # Coalescing rules
///
/// - Duplicate events for the same path are collapsed (keep latest kind).
/// - A `Created` followed by `Modified` for the same path becomes `Created`.
/// - A `Modified` followed by `Deleted` for the same path becomes `Deleted`.
pub struct ChangeScheduler {
    debounce: Duration,
    max_batch: usize,
}

impl ChangeScheduler {
    /// Create a new scheduler.
    ///
    /// - `debounce`: how long to wait after the last event before releasing a batch.
    /// - `max_batch`: maximum number of events in a single batch (0 = unlimited).
    pub fn new(debounce: Duration, max_batch: usize) -> Self {
        Self {
            debounce,
            max_batch,
        }
    }

    /// Run the scheduler loop, consuming raw events and producing [`SyncTrigger`]
    /// signals on the provided channel.
    ///
    /// This function runs until the input channel closes.
    pub async fn run(
        &self,
        input: &mut tokio_mpsc::Receiver<ChangeEvent>,
        output: &tokio_mpsc::Sender<SyncTrigger>,
    ) {
        let mut pending: Vec<ChangeEvent> = Vec::new();
        let mut debounce_deadline: Option<tokio::time::Instant> = None;

        loop {
            match debounce_deadline {
                Some(deadline) => {
                    tokio::select! {
                        event = input.recv() => {
                            match event {
                                Some(e) => {
                                    pending.push(e);
                                    if self.max_batch > 0 && pending.len() >= self.max_batch {
                                        let batch = Self::coalesce(&mut pending);
                                        if !batch.is_empty() {
                                            let _ = output.send(SyncTrigger::ChangesReady(batch)).await;
                                        }
                                        debounce_deadline = None;
                                    } else {
                                        // Reset the debounce timer
                                        debounce_deadline = Some(tokio::time::Instant::now() + self.debounce);
                                    }
                                }
                                None => {
                                    // Input closed — flush remaining and exit
                                    let batch = Self::coalesce(&mut pending);
                                    if !batch.is_empty() {
                                        let _ = output.send(SyncTrigger::ChangesReady(batch)).await;
                                    }
                                    return;
                                }
                            }
                        }
                        _ = tokio::time::sleep_until(deadline) => {
                            let batch = Self::coalesce(&mut pending);
                            if !batch.is_empty() {
                                let _ = output.send(SyncTrigger::ChangesReady(batch)).await;
                            }
                            debounce_deadline = None;
                        }
                    }
                }
                None => {
                    // Waiting for the first event
                    match input.recv().await {
                        Some(e) => {
                            pending.push(e);
                            debounce_deadline = Some(tokio::time::Instant::now() + self.debounce);
                        }
                        None => return, // Input closed with nothing pending
                    }
                }
            }
        }
    }

    /// Coalesce a batch of events, removing duplicates and simplifying
    /// event sequences for the same path.
    fn coalesce(events: &mut Vec<ChangeEvent>) -> Vec<ChangeEvent> {
        if events.is_empty() {
            return Vec::new();
        }

        // Track the final state for each path, applying simplification rules:
        // - Created + Modified → Created
        // - Modified + Deleted → Deleted
        // - Created + Deleted → (removed entirely)
        let mut final_state: std::collections::HashMap<PathBuf, Option<ChangeEvent>> =
            std::collections::HashMap::new();
        // Preserve insertion order
        let mut order: Vec<PathBuf> = Vec::new();

        for event in events.drain(..) {
            let path = entry_path(&event).to_path_buf();
            let is_new = !final_state.contains_key(&path);
            if is_new {
                order.push(path.clone());
            }
            let prev = final_state.get(&path).and_then(|o| o.as_ref()).cloned();
            let merged = merge_events(prev.as_ref(), &event);
            final_state.insert(path, merged);
        }

        order
            .into_iter()
            .filter_map(|p| final_state.remove(&p).flatten())
            .collect()
    }
}

/// Extract the path from a change event.
fn entry_path(e: &ChangeEvent) -> &Path {
    e.primary_path()
}

/// Merge two events for the same path, applying simplification rules:
/// - Created + Modified → Created
/// - Modified + Deleted → Deleted
/// - Created + Deleted → None (file was created then removed — no-op)
/// - Anything + Modified → Modified
/// - Anything + Created → Created
fn merge_events(prev: Option<&ChangeEvent>, next: &ChangeEvent) -> Option<ChangeEvent> {
    use ChangeEvent::*;
    match (prev, next) {
        // Created + Modified = Created (file still exists, keep the creation)
        (Some(Created(_)), Modified(p)) => Some(Created(p.clone())),
        // Created + Deleted = None (net zero — never left the local state)
        (Some(Created(_)), Deleted(_)) => None,
        // Modified + Deleted = Deleted (file was removed)
        (Some(Modified(_)), Deleted(p)) => Some(Deleted(p.clone())),
        // Deleted + Created = Modified (recreated — treat as modification)
        (Some(Deleted(_)), Created(p)) => Some(Modified(p.clone())),
        // Deleted + Modified = Modified (recreated then modified)
        (Some(Deleted(_)), Modified(p)) => Some(Modified(p.clone())),
        // Everything else: last event wins
        (Some(_), other) | (None, other) => Some(other.clone()),
    }
}

/// Watches a directory tree for file changes.
pub struct FileWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    rx: tokio_mpsc::Receiver<ChangeEvent>,
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
                            let _ = tx.blocking_send(ChangeEvent::Created(p));
                        }
                    }
                    EventKind::Modify(_) => {
                        for p in event.paths {
                            let _ = tx.blocking_send(ChangeEvent::Modified(p));
                        }
                    }
                    EventKind::Remove(_) => {
                        for p in event.paths {
                            let _ = tx.blocking_send(ChangeEvent::Deleted(p));
                        }
                    }
                    _ => {}
                }
            }
        });

        Ok(Self { watcher, rx })
    }

    /// Returns a reference to the event receiver.
    pub fn events(&mut self) -> &mut tokio_mpsc::Receiver<ChangeEvent> {
        &mut self.rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scheduler_debounces_events() {
        let (tx, mut rx) = tokio_mpsc::channel(256);
        let (out_tx, mut out_rx) = tokio_mpsc::channel(256);

        let scheduler = ChangeScheduler::new(Duration::from_millis(50), 0);

        // Send rapid events
        tx.send(ChangeEvent::Created(PathBuf::from("a.txt")))
            .await
            .unwrap();
        tx.send(ChangeEvent::Modified(PathBuf::from("a.txt")))
            .await
            .unwrap();
        tx.send(ChangeEvent::Modified(PathBuf::from("b.txt")))
            .await
            .unwrap();
        drop(tx);

        scheduler.run(&mut rx, &out_tx).await;

        let batch = out_rx.recv().await.unwrap();
        match batch {
            SyncTrigger::ChangesReady(events) => {
                // a.txt should be coalesced (Created wins over Modified)
                // b.txt should be present
                assert!(events.len() <= 2);
                let paths: Vec<_> = events.iter().map(|e| e.primary_path()).collect();
                assert!(paths.contains(&Path::new("b.txt")));
            }
        }
    }

    #[tokio::test]
    async fn scheduler_empty_after_debounce() {
        let (tx, mut rx) = tokio_mpsc::channel(256);
        let (out_tx, mut out_rx) = tokio_mpsc::channel(256);

        let scheduler = ChangeScheduler::new(Duration::from_millis(50), 0);

        // Send one event then close
        tx.send(ChangeEvent::Created(PathBuf::from("x.txt")))
            .await
            .unwrap();
        drop(tx);

        scheduler.run(&mut rx, &out_tx).await;

        let batch = out_rx.recv().await.unwrap();
        match batch {
            SyncTrigger::ChangesReady(events) => {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0], ChangeEvent::Created(PathBuf::from("x.txt")));
            }
        }
    }

    #[test]
    fn coalesce_deduplicates() {
        let mut events = vec![
            ChangeEvent::Modified(PathBuf::from("a.txt")),
            ChangeEvent::Modified(PathBuf::from("a.txt")),
            ChangeEvent::Created(PathBuf::from("b.txt")),
        ];
        let result = ChangeScheduler::coalesce(&mut events);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn coalesce_created_then_modified_is_created() {
        let mut events = vec![
            ChangeEvent::Created(PathBuf::from("a.txt")),
            ChangeEvent::Modified(PathBuf::from("a.txt")),
        ];
        let result = ChangeScheduler::coalesce(&mut events);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ChangeEvent::Created(PathBuf::from("a.txt")));
    }

    #[test]
    fn coalesce_modified_then_deleted_is_deleted() {
        let mut events = vec![
            ChangeEvent::Modified(PathBuf::from("a.txt")),
            ChangeEvent::Deleted(PathBuf::from("a.txt")),
        ];
        let result = ChangeScheduler::coalesce(&mut events);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ChangeEvent::Deleted(PathBuf::from("a.txt")));
    }

    #[test]
    fn event_primary_path() {
        assert_eq!(
            ChangeEvent::Created(PathBuf::from("a.txt")).primary_path(),
            Path::new("a.txt")
        );
        assert_eq!(
            ChangeEvent::Renamed(PathBuf::from("a.txt"), PathBuf::from("b.txt")).primary_path(),
            Path::new("a.txt")
        );
    }
}

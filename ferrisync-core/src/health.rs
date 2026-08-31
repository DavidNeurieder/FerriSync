//! Semantic status projections shared by every presentation layer.
//!
//! These are *derived views* of already-persisted state (devices, folders,
//! conflicts, sessions) — the shared vocabulary (`Presence`, `FolderHealth`)
//! that the CLI, the REPL and the Flutter app all speak. This module only
//! decides *what a state is*; wording, colors and layout stay in the callers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage::Storage;

/// A device that pinged us within this many seconds is `Connected`.
/// Kept in sync with the window the Flutter app already uses.
pub const CONNECTED_WINDOW_SECS: i64 = 300;
/// After this much silence a device is `Offline` rather than `RecentlySeen`.
pub const RECENTLY_SEEN_WINDOW_SECS: i64 = 24 * 60 * 60;

/// How recently a paired device was last heard from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Connected,
    RecentlySeen,
    Offline,
}

impl Presence {
    pub fn from_last_seen(now_secs: i64, last_seen: Option<i64>) -> Self {
        match last_seen {
            Some(ts) if now_secs - ts <= CONNECTED_WINDOW_SECS => Presence::Connected,
            Some(ts) if now_secs - ts <= RECENTLY_SEEN_WINDOW_SECS => Presence::RecentlySeen,
            _ => Presence::Offline,
        }
    }

    /// Stable machine-readable label (CLI `--json`, FRB).
    pub fn as_str(self) -> &'static str {
        match self {
            Presence::Connected => "connected",
            Presence::RecentlySeen => "recently_seen",
            Presence::Offline => "offline",
        }
    }
}

/// What a configured folder currently means for the user.
///
/// Derivation rules (see [`folder_health`]):
/// - `NotConfigured` — the folder points at this device itself (served
///   locally, not yet attached to a remote).
/// - `Conflict` — the folder has ≥ 1 unresolved conflict backup.
/// - `Syncing` — the folder is transferring right now.
/// - `Error` — the last sync attempt for it failed.
/// - `Healthy` — peer is connected and the folder has synced before.
/// - `Waiting` — peer is around but not connected, or never synced yet.
/// - `Offline` — peer fell off the network before the first sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderHealth {
    Healthy,
    Syncing,
    Waiting,
    Offline,
    Error,
    Conflict,
    NotConfigured,
}

impl FolderHealth {
    pub fn as_str(self) -> &'static str {
        match self {
            FolderHealth::Healthy => "healthy",
            FolderHealth::Syncing => "syncing",
            FolderHealth::Waiting => "waiting",
            FolderHealth::Offline => "offline",
            FolderHealth::Error => "error",
            FolderHealth::Conflict => "conflict",
            FolderHealth::NotConfigured => "not_configured",
        }
    }

    /// Healths that should surface in a `needs attention` section.
    pub fn needs_attention(self) -> bool {
        matches!(
            self,
            FolderHealth::Conflict | FolderHealth::Error | FolderHealth::Offline
        )
    }
}

/// One paired device with its derived presence and folder count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub id: String,
    pub name: String,
    pub presence: Presence,
    /// Number of configured sync folders pointing at this device.
    pub folder_count: usize,
}

/// One configured sync folder plus its derived health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderStatus {
    pub id: i64,
    pub path: String,
    pub device_id: String,
    /// Display name of the peer device, when the row still exists.
    pub device_name: Option<String>,
    pub health: FolderHealth,
    pub last_sync: Option<i64>,
    /// Number of unresolved conflict backups in this folder.
    pub conflicts: usize,
}

impl FolderStatus {
    /// The folder's peer label for presentation ("Pixel 9", the device id,
    /// or "this device" for locally-hosted rows).
    pub fn peer_label(&self, own_device_id: &str) -> String {
        if self.device_id == own_device_id {
            "this device".to_string()
        } else {
            self.device_name
                .clone()
                .unwrap_or_else(|| self.device_id.clone())
        }
    }
}

/// Roll-up used by `status` summaries, the REPL banner and `package.json`-ish
/// health endpoints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthSummary {
    pub device_total: usize,
    pub device_connected: usize,
    pub device_recently_seen: usize,
    pub folders_total: usize,
    pub folders_healthy: usize,
    /// Folders that need attention: conflict/error/offline.
    pub folders_needing_attention: usize,
    pub conflicts_total: usize,
    /// Most recent sync across all folders, unix seconds.
    pub last_sync_secs: Option<i64>,
}

impl HealthSummary {
    pub fn needs_attention(&self) -> bool {
        self.conflicts_total > 0 || self.folders_needing_attention > 0
    }
}

/// Transient state a caller knows about that storage cannot see. Passed in so
/// the pure projection stays testable and storage-agnostic.
#[derive(Debug, Default, Clone)]
pub struct LiveState {
    /// Local paths of folders currently transferring.
    pub syncing: Vec<String>,
    /// Local paths of folders whose most recent sync attempt errored.
    pub errored: Vec<String>,
}

/// A single derived view of devices + folders + health, computed in one pass
/// (conflict scanning is the only expensive step, so avoid doing it 3×).
#[derive(Debug, Clone)]
pub struct HealthSnapshot {
    pub devices: Vec<DeviceStatus>,
    pub folders: Vec<FolderStatus>,
    pub summary: HealthSummary,
}

/// Current unix time in seconds.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Everything derived in a single snapshot. Use this when you need the whole
/// picture (status, doctor, the Flutter dashboard).
pub fn snapshot(
    storage: &Storage,
    own_device_id: &str,
    now: i64,
    live: &LiveState,
) -> anyhow::Result<HealthSnapshot> {
    let devices = compute_device_statuses(storage, own_device_id, now)?;
    let folders = compute_folder_statuses(storage, own_device_id, now, live)?;
    let summary = summarize(&devices, &folders);
    Ok(HealthSnapshot {
        devices,
        folders,
        summary,
    })
}

/// Derive presence + folder counts for every paired device,
/// excluding our own row (created when a folder is served here).
pub fn compute_device_statuses(
    storage: &Storage,
    own_device_id: &str,
    now: i64,
) -> anyhow::Result<Vec<DeviceStatus>> {
    let devices = storage.list_devices()?;
    let folders = storage.list_sync_folders()?;
    let mut folder_counts: HashMap<&str, usize> = HashMap::new();
    for (_, _path, dev, _, _last) in &folders {
        *folder_counts.entry(dev.as_str()).or_default() += 1;
    }

    let mut out: Vec<DeviceStatus> = devices
        .into_iter()
        .filter(|(id, _, _)| id != own_device_id)
        .map(|(id, name, last_seen)| DeviceStatus {
            presence: Presence::from_last_seen(now, last_seen),
            folder_count: folder_counts.get(id.as_str()).copied().unwrap_or(0),
            id,
            name,
        })
        .collect();

    out.sort_by_key(|d| (d.presence != Presence::Connected, d.name.clone()));
    Ok(out)
}

/// Per-folder conflict counts. Scans the sync-root directories for backups,
/// so it is the expensive part of a snapshot — compute it once.
pub fn folder_conflict_counts(storage: &Storage) -> anyhow::Result<HashMap<i64, usize>> {
    let mut out: HashMap<i64, usize> = HashMap::new();
    for entry in crate::sync_engine::conflicts::list_conflicts(storage)? {
        *out.entry(entry.folder_id).or_default() += 1;
    }
    Ok(out)
}

/// Every configured folder with its peer label and derived health.
pub fn compute_folder_statuses(
    storage: &Storage,
    own_device_id: &str,
    now: i64,
    live: &LiveState,
) -> anyhow::Result<Vec<FolderStatus>> {
    let folders = storage.list_sync_folders()?;
    let devices = storage.list_devices()?;

    let device_name: HashMap<&str, &str> = devices
        .iter()
        .map(|(id, name, _)| (id.as_str(), name.as_str()))
        .collect();
    let presence: HashMap<&str, Presence> = devices
        .iter()
        .map(|(id, _, last)| (id.as_str(), Presence::from_last_seen(now, *last)))
        .collect();
    let conflicts = folder_conflict_counts(storage)?;

    let syncing: Vec<&str> = live.syncing.iter().map(String::as_str).collect();
    let errored: Vec<&str> = live.errored.iter().map(String::as_str).collect();

    let mut out: Vec<FolderStatus> = folders
        .into_iter()
        .map(|(id, path, dev_id, _dir, last_sync)| {
            let count = conflicts.get(&id).copied().unwrap_or(0);
            let dev_presence = presence.get(dev_id.as_str()).copied().unwrap_or(Presence::Offline);
            FolderStatus {
                health: folder_health(
                    dev_id.as_str(),
                    own_device_id,
                    dev_presence,
                    last_sync,
                    count,
                    syncing.contains(&path.as_str()),
                    errored.contains(&path.as_str()),
                ),
                id,
                conflicts: count,
                device_name: device_name.get(dev_id.as_str()).map(|n| n.to_string()),
                path,
                device_id: dev_id,
                last_sync,
            }
        })
        .collect();

    out.sort_by_key(|f| f.path.clone());
    Ok(out)
}

/// The derivation rules behind [`FolderHealth`]. Documented above the enum;
/// order matters — an unresolved conflict wins over "currently syncing".
pub fn folder_health(
    device_id: &str,
    own_device_id: &str,
    dev_presence: Presence,
    last_sync: Option<i64>,
    conflicts: usize,
    is_syncing: bool,
    is_errored: bool,
) -> FolderHealth {
    if device_id == own_device_id {
        return FolderHealth::NotConfigured;
    }
    if conflicts > 0 {
        return FolderHealth::Conflict;
    }
    if is_syncing {
        return FolderHealth::Syncing;
    }
    if is_errored {
        return FolderHealth::Error;
    }
    match dev_presence {
        Presence::Connected => {
            if last_sync.is_some() {
                FolderHealth::Healthy
            } else {
                FolderHealth::Waiting
            }
        }
        Presence::RecentlySeen => FolderHealth::Waiting,
        Presence::Offline => {
            if last_sync.is_none() {
                FolderHealth::Offline
            } else {
                FolderHealth::Waiting
            }
        }
    }
}

/// Aggregate counts for summaries and banners.
pub fn summarize(devices: &[DeviceStatus], folders: &[FolderStatus]) -> HealthSummary {
    HealthSummary {
        device_total: devices.len(),
        device_connected: devices
            .iter()
            .filter(|d| d.presence == Presence::Connected)
            .count(),
        device_recently_seen: devices
            .iter()
            .filter(|d| d.presence == Presence::RecentlySeen)
            .count(),
        folders_total: folders.len(),
        folders_healthy: folders
            .iter()
            .filter(|f| matches!(f.health, FolderHealth::Healthy | FolderHealth::Syncing))
            .count(),
        folders_needing_attention: folders
            .iter()
            .filter(|f| f.health.needs_attention())
            .count(),
        conflicts_total: folders.iter().map(|f| f.conflicts).sum(),
        last_sync_secs: folders.iter().filter_map(|f| f.last_sync).max(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn storage() -> Storage {
        Storage::open(&PathBuf::from(":memory:")).unwrap()
    }

    #[test]
    fn presence_thresholds() {
        let now = 1_000_000;
        assert_eq!(
            Presence::from_last_seen(now, Some(now)),
            Presence::Connected
        );
        assert_eq!(
            Presence::from_last_seen(now, Some(now - CONNECTED_WINDOW_SECS)),
            Presence::Connected
        );
        assert_eq!(
            Presence::from_last_seen(now, Some(now - CONNECTED_WINDOW_SECS - 1)),
            Presence::RecentlySeen
        );
        assert_eq!(
            Presence::from_last_seen(now, Some(now - RECENTLY_SEEN_WINDOW_SECS)),
            Presence::RecentlySeen
        );
        assert_eq!(
            Presence::from_last_seen(now, Some(now - RECENTLY_SEEN_WINDOW_SECS - 1)),
            Presence::Offline
        );
        assert_eq!(Presence::from_last_seen(now, None), Presence::Offline);
    }

    #[test]
    fn folder_health_rules() {
        let own = "self";
        // Not configured: pointing at ourselves.
        assert_eq!(
            folder_health("self", own, Presence::Connected, None, 0, false, false),
            FolderHealth::NotConfigured
        );
        // Conflicts beat everything.
        assert_eq!(
            folder_health("peer", own, Presence::Connected, Some(1), 2, false, false),
            FolderHealth::Conflict
        );
        assert_eq!(
            folder_health("peer", own, Presence::Offline, None, 1, false, false),
            FolderHealth::Conflict
        );
        // Syncing beats presence.
        assert_eq!(
            folder_health("peer", own, Presence::Offline, Some(1), 0, true, false),
            FolderHealth::Syncing
        );
        // Errors beat presence.
        assert_eq!(
            folder_health("peer", own, Presence::Connected, Some(1), 0, false, true),
            FolderHealth::Error
        );
        // Connected + synced before → healthy.
        assert_eq!(
            folder_health("peer", own, Presence::Connected, Some(1), 0, false, false),
            FolderHealth::Healthy
        );
        // Connected but never synced → waiting.
        assert_eq!(
            folder_health("peer", own, Presence::Connected, None, 0, false, false),
            FolderHealth::Waiting
        );
        // Recently seen → waiting.
        assert_eq!(
            folder_health("peer", own, Presence::RecentlySeen, Some(1), 0, false, false),
            FolderHealth::Waiting
        );
        // Offline + synced before → waiting.
        assert_eq!(
            folder_health("peer", own, Presence::Offline, Some(1), 0, false, false),
            FolderHealth::Waiting
        );
        // Offline + never synced → offline.
        assert_eq!(
            folder_health("peer", own, Presence::Offline, None, 0, false, false),
            FolderHealth::Offline
        );
        // Unknown device id treats as offline.
        assert_eq!(
            folder_health("ghost", own, Presence::Offline, None, 0, false, false),
            FolderHealth::Offline
        );
    }

    #[test]
    fn device_statuses_exclude_own_and_count_folders() {
        let s = storage();
        s.upsert_device("self", "me", None, None).unwrap();
        s.upsert_device("peer", "Pixel 9", None, Some("1.2.3.4:5".into()))
            .unwrap();
        s.add_sync_folder("/a", "peer", "bidirectional").unwrap();
        s.add_sync_folder("/b", "peer", "pull").unwrap();
        s.add_sync_folder("/c", "self", "bidirectional").unwrap();

        let now = now_secs();
        let devs = compute_device_statuses(&s, "self", now).unwrap();
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].id, "peer");
        assert_eq!(devs[0].folder_count, 2);
        assert_eq!(devs[0].presence, Presence::Connected);
    }

    #[test]
    fn folder_statuses_attach_peer_names() {
        let s = storage();
        s.upsert_device("self", "me", None, None).unwrap();
        s.upsert_device("peer", "Laptop", None, None).unwrap();
        s.add_sync_folder("/a", "peer", "bidirectional").unwrap();
        s.set_folder_last_sync(1, 100).unwrap();

        // Upsert stamps last_seen = now, so the peer is Connected; the folder
        // has synced before → Healthy, with the peer's display name attached.
        let now = now_secs();
        let fs = compute_folder_statuses(&s, "self", now, &LiveState::default()).unwrap();
        assert_eq!(fs.len(), 1);
        assert_eq!(fs[0].device_name.as_deref(), Some("Laptop"));
        assert_eq!(fs[0].peer_label("self"), "Laptop");
        assert_eq!(fs[0].health, FolderHealth::Healthy);
    }

    #[test]
    fn snapshot_summary_rolls_up() {
        let devices = vec![
            DeviceStatus {
                id: "a".into(),
                name: "A".into(),
                presence: Presence::Connected,
                folder_count: 1,
            },
            DeviceStatus {
                id: "b".into(),
                name: "B".into(),
                presence: Presence::Offline,
                folder_count: 0,
            },
        ];
        let folders = vec![
            FolderStatus {
                id: 1,
                path: "/a".into(),
                device_id: "a".into(),
                device_name: Some("A".into()),
                health: FolderHealth::Healthy,
                last_sync: Some(500),
                conflicts: 0,
            },
            FolderStatus {
                id: 2,
                path: "/b".into(),
                device_id: "a".into(),
                device_name: Some("A".into()),
                health: FolderHealth::Offline,
                last_sync: None,
                conflicts: 0,
            },
            FolderStatus {
                id: 3,
                path: "/c".into(),
                device_id: "a".into(),
                device_name: Some("A".into()),
                health: FolderHealth::Conflict,
                last_sync: Some(600),
                conflicts: 2,
            },
        ];

        let summary = summarize(&devices, &folders);
        assert_eq!(summary.device_total, 2);
        assert_eq!(summary.device_connected, 1);
        assert_eq!(summary.device_recently_seen, 0);
        assert_eq!(summary.folders_total, 3);
        assert_eq!(summary.folders_healthy, 1);
        assert_eq!(summary.folders_needing_attention, 2);
        assert_eq!(summary.conflicts_total, 2);
        assert_eq!(summary.last_sync_secs, Some(600));
        assert!(summary.needs_attention());
    }
}
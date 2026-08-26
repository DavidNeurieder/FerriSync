use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::Result;
use async_trait::async_trait;

use crate::domain::{
    CertificateFingerprint, Device, DeviceId, FileMetadata, FilePath, Folder, FolderId, Tombstone,
};

use super::traits::{DeviceCleanup, HistoryEntry, SessionRecord, StateStore};

/// In-memory implementation of `StateStore` for testing.
#[derive(Debug, Default)]
pub struct InMemoryStateStore {
    devices: RwLock<HashMap<DeviceId, Device>>,
    folders: RwLock<HashMap<FolderId, Folder>>,
    file_metadata: RwLock<HashMap<(FolderId, FilePath), FileMetadata>>,
    history: RwLock<Vec<HistoryEntry>>,
    tombstones: RwLock<Vec<Tombstone>>,
    sessions: RwLock<Vec<SessionRecord>>,
    next_folder_id: RwLock<i64>,
}

impl InMemoryStateStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl StateStore for InMemoryStateStore {
    // ── Device operations ──

    async fn get_device(&self, id: &DeviceId) -> Result<Option<Device>> {
        Ok(self.devices.read().unwrap().get(id).cloned())
    }

    async fn upsert_device(&self, device: &Device) -> Result<()> {
        self.devices
            .write()
            .unwrap()
            .insert(device.id.clone(), device.clone());
        Ok(())
    }

    async fn list_devices(&self) -> Result<Vec<Device>> {
        Ok(self
            .devices
            .read()
            .unwrap()
            .values()
            .cloned()
            .collect())
    }

    async fn remove_device(&self, id: &DeviceId) -> Result<DeviceCleanup> {
        let mut devices = self.devices.write().unwrap();
        let mut folders = self.folders.write().unwrap();
        let mut metadata = self.file_metadata.write().unwrap();
        let mut history = self.history.write().unwrap();
        let mut sessions = self.sessions.write().unwrap();

        let device_removed = devices.remove(id).is_some() as usize;

        let folder_ids: Vec<FolderId> = folders
            .values()
            .filter(|f| f.device_id == *id)
            .map(|f| f.id)
            .collect();

        let mut folders_removed = 0;
        for fid in &folder_ids {
            if folders.remove(fid).is_some() {
                folders_removed += 1;
            }
        }

        let mut metadata_removed = 0;
        metadata.retain(|(fid, _), _| {
            if folder_ids.contains(fid) {
                metadata_removed += 1;
                false
            } else {
                true
            }
        });

        let history_before = history.len();
        history.retain(|e| e.device_id != *id && !folder_ids.contains(&e.folder_id));
        let history_removed = history_before - history.len();

        let sessions_before = sessions.len();
        sessions.retain(|s| s.peer_device != id.0);
        let sessions_removed = sessions_before - sessions.len();

        Ok(DeviceCleanup {
            sessions_removed,
            history_removed,
            metadata_removed,
            folders_removed,
            device_removed,
        })
    }

    async fn get_device_cert(&self, id: &DeviceId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .devices
            .read()
            .unwrap()
            .get(id)
            .and_then(|d| d.fingerprint.as_ref())
            .map(|fp| fp.0.clone()))
    }

    async fn set_device_cert(&self, id: &DeviceId, cert_der: &[u8]) -> Result<()> {
        let fp = CertificateFingerprint::from_der(cert_der);
        let mut devices = self.devices.write().unwrap();
        if let Some(device) = devices.get_mut(id) {
            device.fingerprint = Some(fp);
        } else {
            devices.insert(
                id.clone(),
                Device {
                    id: id.clone(),
                    name: String::new(),
                    fingerprint: Some(fp),
                    last_seen: None,
                    last_addr: None,
                },
            );
        }
        Ok(())
    }

    async fn get_device_by_cert_fingerprint(
        &self,
        fingerprint: &CertificateFingerprint,
    ) -> Result<Option<DeviceId>> {
        Ok(self
            .devices
            .read()
            .unwrap()
            .values()
            .find(|d| d.fingerprint.as_ref() == Some(fingerprint))
            .map(|d| d.id.clone()))
    }

    async fn device_last_addr(&self, id: &DeviceId) -> Result<Option<String>> {
        Ok(self
            .devices
            .read()
            .unwrap()
            .get(id)
            .and_then(|d| d.last_addr.clone()))
    }

    async fn set_device_last_addr(&self, id: &DeviceId, addr: &str) -> Result<()> {
        if let Some(device) = self.devices.write().unwrap().get_mut(id) {
            device.last_addr = Some(addr.to_string());
        }
        Ok(())
    }

    // ── Folder operations ──

    async fn get_folder(&self, id: FolderId) -> Result<Option<Folder>> {
        Ok(self.folders.read().unwrap().get(&id).cloned())
    }

    async fn list_folders(&self) -> Result<Vec<Folder>> {
        let mut folders: Vec<Folder> = self.folders.read().unwrap().values().cloned().collect();
        folders.sort_by_key(|f| f.id.0);
        Ok(folders)
    }

    async fn add_folder(
        &self,
        local_path: &str,
        device_id: &DeviceId,
        direction: &str,
    ) -> Result<FolderId> {
        let mut next_id = self.next_folder_id.write().unwrap();
        *next_id += 1;
        let id = FolderId(*next_id);
        let folder = Folder {
            id,
            local_path: local_path.to_string(),
            device_id: device_id.clone(),
            direction: direction.parse().unwrap_or(crate::domain::SyncDirection::Bidirectional),
            last_sync_at: None,
        };
        self.folders.write().unwrap().insert(id, folder);
        Ok(id)
    }

    async fn remove_folder(&self, id: FolderId) -> Result<()> {
        self.folders.write().unwrap().remove(&id);
        Ok(())
    }

    async fn set_folder_last_sync(&self, id: FolderId, ts: i64) -> Result<()> {
        if let Some(folder) = self.folders.write().unwrap().get_mut(&id) {
            folder.last_sync_at = Some(ts);
        }
        Ok(())
    }

    async fn set_folder_device(&self, id: FolderId, device_id: &DeviceId) -> Result<()> {
        if let Some(folder) = self.folders.write().unwrap().get_mut(&id) {
            folder.device_id = device_id.clone();
        }
        Ok(())
    }

    // ── File metadata ──

    async fn get_file_metadata(
        &self,
        folder_id: FolderId,
        path: &FilePath,
    ) -> Result<Option<FileMetadata>> {
        Ok(self
            .file_metadata
            .read()
            .unwrap()
            .get(&(folder_id, path.clone()))
            .cloned())
    }

    async fn upsert_file_metadata(&self, meta: &FileMetadata) -> Result<()> {
        let key = (
            meta.folder_id,
            FilePath(meta.path.0.clone()),
        );
        self.file_metadata
            .write()
            .unwrap()
            .insert(key, meta.clone());
        Ok(())
    }

    // ── History ──

    async fn record_history(&self, entry: &HistoryEntry) -> Result<()> {
        self.history.write().unwrap().push(entry.clone());
        Ok(())
    }

    // ── Tombstones ──

    async fn get_tombstones(&self, folder_id: FolderId, since: u64) -> Result<Vec<Tombstone>> {
        Ok(self
            .tombstones
            .read()
            .unwrap()
            .iter()
            .filter(|t| t.folder_id == folder_id && t.deleted_at >= since)
            .cloned()
            .collect())
    }

    async fn add_tombstone(&self, tomb: &Tombstone) -> Result<()> {
        self.tombstones.write().unwrap().push(tomb.clone());
        Ok(())
    }

    // ── Sessions ──

    async fn record_session(&self, session: &SessionRecord) -> Result<()> {
        self.sessions.write().unwrap().push(session.clone());
        Ok(())
    }

    async fn list_recent_sessions(&self, limit: u32) -> Result<Vec<SessionRecord>> {
        let sessions = self.sessions.read().unwrap();
        Ok(sessions
            .iter()
            .rev()
            .take(limit as usize)
            .cloned()
            .collect())
    }

    // ── Bulk operations ──

    async fn clear_all_sync_state(&self) -> Result<(usize, usize)> {
        let folders = self.folders.read().unwrap().len();
        let devices = self.devices.read().unwrap().len();
        self.folders.write().unwrap().clear();
        self.devices.write().unwrap().clear();
        self.file_metadata.write().unwrap().clear();
        self.history.write().unwrap().clear();
        Ok((folders, devices))
    }

    async fn remove_sync_folders(
        &self,
        local_path: &str,
        device_id: Option<&DeviceId>,
    ) -> Result<usize> {
        let mut folders = self.folders.write().unwrap();
        let before = folders.len();
        folders.retain(|_, f| {
            if f.local_path != local_path {
                return true;
            }
            if let Some(dev) = device_id {
                return f.device_id != *dev;
            }
            false
        });
        Ok(before - folders.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn device_crud() {
        let store = InMemoryStateStore::new();
        let dev = Device {
            id: DeviceId("dev-1".into()),
            name: "Laptop".into(),
            fingerprint: None,
            last_seen: None,
            last_addr: None,
        };

        store.upsert_device(&dev).await.unwrap();
        let got = store.get_device(&DeviceId("dev-1".into())).await.unwrap();
        assert_eq!(got.as_ref().map(|d| &d.name), Some(&"Laptop".to_string()));

        let list = store.list_devices().await.unwrap();
        assert_eq!(list.len(), 1);

        let cleanup = store.remove_device(&DeviceId("dev-1".into())).await.unwrap();
        assert_eq!(cleanup.device_removed, 1);
        assert!(store.get_device(&DeviceId("dev-1".into())).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn folder_crud() {
        let store = InMemoryStateStore::new();
        let dev = Device {
            id: DeviceId("dev-1".into()),
            name: "Test".into(),
            fingerprint: None,
            last_seen: None,
            last_addr: None,
        };
        store.upsert_device(&dev).await.unwrap();

        let id = store
            .add_folder("/sync", &DeviceId("dev-1".into()), "bidirectional")
            .await
            .unwrap();
        assert_eq!(id.0, 1);

        let folders = store.list_folders().await.unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].local_path, "/sync");

        store.set_folder_last_sync(id, 1000).await.unwrap();
        let f = store.get_folder(id).await.unwrap().unwrap();
        assert_eq!(f.last_sync_at, Some(1000));

        store.remove_folder(id).await.unwrap();
        assert!(store.get_folder(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn file_metadata_crud() {
        use crate::domain::FileHash;
        let store = InMemoryStateStore::new();
        let meta = FileMetadata {
            path: FilePath("test.txt".into()),
            folder_id: FolderId(1),
            kind: crate::domain::file::EntryKind::File,
            mtime: 100,
            size: 50,
            hash: FileHash([1u8; 32]),
            device_id: DeviceId("dev-1".into()),
            version: 0,
            local_version: 0,
            remote_version: 0,
            local_mtime: 0,
            remote_mtime: 0,
        };

        store.upsert_file_metadata(&meta).await.unwrap();
        let got = store
            .get_file_metadata(FolderId(1), &FilePath("test.txt".into()))
            .await
            .unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().size, 50);
    }

    #[tokio::test]
    async fn tombstones() {
        let store = InMemoryStateStore::new();
        let tomb = Tombstone {
            path: FilePath("del.txt".into()),
            folder_id: FolderId(1),
            device_id: DeviceId("dev-1".into()),
            deleted_at: 5,
        };
        store.add_tombstone(&tomb).await.unwrap();

        let all = store.get_tombstones(FolderId(1), 0).await.unwrap();
        assert_eq!(all.len(), 1);

        let recent = store.get_tombstones(FolderId(1), 6).await.unwrap();
        assert!(recent.is_empty());
    }

    #[tokio::test]
    async fn session_record_and_list() {
        let store = InMemoryStateStore::new();
        let rec = SessionRecord {
            ts: 100,
            direction: "outgoing".into(),
            peer_device: "dev-1".into(),
            addr: "192.168.1.5:9847".into(),
            folder_path: "/sync".into(),
            pushed_count: 3,
            pulled_count: 2,
            conflicts_count: 1,
        };
        store.record_session(&rec).await.unwrap();

        let sessions = store.list_recent_sessions(10).await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].pushed_count, 3);
    }
}

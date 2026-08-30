use anyhow::Result;
use async_trait::async_trait;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::domain::{
    CertificateFingerprint, Device, DeviceId, EntryKind, FileMetadata, FilePath, Folder, FolderId,
    Tombstone,
};

use super::traits::{DeviceCleanup, HistoryEntry, SessionRecord, StateStore};

/// SQLite-backed implementation of `StateStore`.
pub struct SqliteStateStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStateStore {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.initialize_schema()?;
        Ok(store)
    }

    fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                cert_der BLOB,
                last_seen INTEGER,
                last_addr TEXT
            );

            CREATE TABLE IF NOT EXISTS sync_folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                local_path TEXT NOT NULL,
                device_id TEXT NOT NULL,
                active BOOL DEFAULT 1,
                direction TEXT DEFAULT 'bidirectional',
                last_sync_at INTEGER,
                FOREIGN KEY (device_id) REFERENCES devices(id)
            );

            CREATE TABLE IF NOT EXISTS file_metadata (
                path TEXT NOT NULL,
                folder_id INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                size INTEGER NOT NULL,
                hash BLOB,
                device_id TEXT,
                version INTEGER DEFAULT 0,
                local_version INTEGER DEFAULT 0,
                remote_version INTEGER DEFAULT 0,
                local_mtime INTEGER DEFAULT 0,
                remote_mtime INTEGER DEFAULT 0,
                PRIMARY KEY (folder_id, path),
                FOREIGN KEY (folder_id) REFERENCES sync_folders(id)
            );

            CREATE TABLE IF NOT EXISTS file_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_id INTEGER NOT NULL,
                path TEXT NOT NULL,
                device_id TEXT,
                action TEXT NOT NULL,
                version INTEGER,
                mtime INTEGER,
                hash BLOB,
                size INTEGER,
                recorded_at INTEGER NOT NULL,
                FOREIGN KEY (folder_id) REFERENCES sync_folders(id)
            );

            CREATE TABLE IF NOT EXISTS sync_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts INTEGER NOT NULL,
                direction TEXT NOT NULL,
                peer_device TEXT NOT NULL,
                addr TEXT NOT NULL,
                folder_path TEXT NOT NULL,
                pushed_count INTEGER NOT NULL DEFAULT 0,
                pulled_count INTEGER NOT NULL DEFAULT 0,
                conflicts_count INTEGER NOT NULL DEFAULT 0,
                pushed_bytes INTEGER NOT NULL DEFAULT 0,
                pulled_bytes INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS tombstones (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_id INTEGER NOT NULL,
                path TEXT NOT NULL,
                device_id TEXT NOT NULL,
                deleted_at INTEGER NOT NULL,
                FOREIGN KEY (folder_id) REFERENCES sync_folders(id)
            );
            ",
        )?;
        // Migration for databases created before last_addr existed.
        let _ = conn.execute("ALTER TABLE devices ADD COLUMN last_addr TEXT", []);
        // Migration: add tombstones table for older databases.
        let _ = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tombstones (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_id INTEGER NOT NULL,
                path TEXT NOT NULL,
                device_id TEXT NOT NULL,
                deleted_at INTEGER NOT NULL,
                FOREIGN KEY (folder_id) REFERENCES sync_folders(id)
            );",
        );
        // Migrations for databases created before byte accounting existed.
        let _ = conn.execute(
            "ALTER TABLE sync_sessions ADD COLUMN pushed_bytes INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE sync_sessions ADD COLUMN pulled_bytes INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(())
    }
}

#[async_trait]
impl StateStore for SqliteStateStore {
    // ── Device operations ──

    async fn get_device(&self, id: &DeviceId) -> Result<Option<Device>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, cert_der, last_seen, last_addr FROM devices WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id.0])?;
        match rows.next()? {
            Some(row) => Ok(Some(Device {
                id: DeviceId(row.get(0)?),
                name: row.get(1)?,
                fingerprint: row
                    .get::<_, Option<Vec<u8>>>(2)?
                    .map(CertificateFingerprint),
                last_seen: row.get(3)?,
                last_addr: row.get(4)?,
            })),
            None => Ok(None),
        }
    }

    async fn upsert_device(&self, device: &Device) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let cert = device.fingerprint.as_ref().map(|f| &f.0[..]);
        conn.execute(
            "INSERT INTO devices (id, name, cert_der, last_seen, last_addr)
             VALUES (?1, ?2, ?3, unixepoch(), ?4)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               cert_der = COALESCE(excluded.cert_der, devices.cert_der),
               last_seen = unixepoch(),
               last_addr = COALESCE(excluded.last_addr, devices.last_addr)",
            rusqlite::params![device.id.0, device.name, cert, device.last_addr],
        )?;
        Ok(())
    }

    async fn list_devices(&self) -> Result<Vec<Device>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, name, cert_der, last_seen, last_addr FROM devices ORDER BY last_seen DESC")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Device {
                    id: DeviceId(row.get(0)?),
                    name: row.get(1)?,
                    fingerprint: row
                        .get::<_, Option<Vec<u8>>>(2)?
                        .map(CertificateFingerprint),
                    last_seen: row.get(3)?,
                    last_addr: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    async fn remove_device(&self, id: &DeviceId) -> Result<DeviceCleanup> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let sessions = tx.execute(
            "DELETE FROM sync_sessions WHERE peer_device = ?1",
            rusqlite::params![id.0],
        )?;
        let history = tx.execute(
            "DELETE FROM file_history WHERE device_id = ?1 \
             OR folder_id IN (SELECT id FROM sync_folders WHERE device_id = ?1)",
            rusqlite::params![id.0],
        )?;
        let metadata = tx.execute(
            "DELETE FROM file_metadata \
             WHERE folder_id IN (SELECT id FROM sync_folders WHERE device_id = ?1)",
            rusqlite::params![id.0],
        )?;
        let folders = tx.execute(
            "DELETE FROM sync_folders WHERE device_id = ?1",
            rusqlite::params![id.0],
        )?;
        let device = tx.execute(
            "DELETE FROM devices WHERE id = ?1",
            rusqlite::params![id.0],
        )?;
        tx.commit()?;
        Ok(DeviceCleanup {
            sessions_removed: sessions,
            history_removed: history,
            metadata_removed: metadata,
            folders_removed: folders,
            device_removed: device,
        })
    }

    async fn get_device_cert(&self, id: &DeviceId) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT cert_der FROM devices WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id.0])?;
        match rows.next()? {
            Some(row) => Ok(row.get(0)?),
            None => Ok(None),
        }
    }

    async fn set_device_cert(&self, id: &DeviceId, cert_der: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE devices SET cert_der = ?1 WHERE id = ?2",
            rusqlite::params![cert_der, id.0],
        )?;
        Ok(())
    }

    async fn get_device_by_cert_fingerprint(
        &self,
        fingerprint: &CertificateFingerprint,
    ) -> Result<Option<DeviceId>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, cert_der FROM devices WHERE cert_der IS NOT NULL")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<Vec<u8>>>(1)?,
            ))
        })?;
        for row in rows {
            let (id, cert_der) = row?;
            if let Some(der) = cert_der {
                let fp = CertificateFingerprint::from_der(&der);
                if fp == *fingerprint {
                    return Ok(Some(DeviceId(id)));
                }
            }
        }
        Ok(None)
    }

    async fn device_last_addr(&self, id: &DeviceId) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT last_addr FROM devices WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id.0])?;
        match rows.next()? {
            Some(row) => Ok(row.get(0)?),
            None => Ok(None),
        }
    }

    async fn set_device_last_addr(&self, id: &DeviceId, addr: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE devices SET last_addr = ?1 WHERE id = ?2",
            rusqlite::params![addr, id.0],
        )?;
        Ok(())
    }

    // ── Folder operations ──

    async fn get_folder(&self, id: FolderId) -> Result<Option<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, local_path, device_id, direction, last_sync_at FROM sync_folders WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id.0])?;
        match rows.next()? {
            Some(row) => Ok(Some(Folder {
                id: FolderId(row.get(0)?),
                local_path: row.get(1)?,
                device_id: DeviceId(row.get(2)?),
                direction: row.get::<_, String>(3)?.parse().unwrap_or(crate::domain::SyncDirection::Bidirectional),
                last_sync_at: row.get(4)?,
            })),
            None => Ok(None),
        }
    }

    async fn list_folders(&self) -> Result<Vec<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, local_path, device_id, direction, last_sync_at FROM sync_folders ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Folder {
                    id: FolderId(row.get(0)?),
                    local_path: row.get(1)?,
                    device_id: DeviceId(row.get(2)?),
                    direction: row.get::<_, String>(3)?.parse().unwrap_or(crate::domain::SyncDirection::Bidirectional),
                    last_sync_at: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    async fn add_folder(
        &self,
        local_path: &str,
        device_id: &DeviceId,
        direction: &str,
    ) -> Result<FolderId> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_folders (local_path, device_id, direction) VALUES (?1, ?2, ?3)",
            rusqlite::params![local_path, device_id.0, direction],
        )?;
        Ok(FolderId(conn.last_insert_rowid()))
    }

    async fn remove_folder(&self, id: FolderId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM sync_folders WHERE id = ?1",
            rusqlite::params![id.0],
        )?;
        Ok(())
    }

    async fn set_folder_last_sync(&self, id: FolderId, ts: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_folders SET last_sync_at = ?1 WHERE id = ?2",
            rusqlite::params![ts, id.0],
        )?;
        Ok(())
    }

    async fn set_folder_device(&self, id: FolderId, device_id: &DeviceId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_folders SET device_id = ?1 WHERE id = ?2",
            rusqlite::params![device_id.0, id.0],
        )?;
        Ok(())
    }

    // ── File metadata ──

    async fn get_file_metadata(
        &self,
        folder_id: FolderId,
        path: &FilePath,
    ) -> Result<Option<FileMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT path, folder_id, mtime, size, COALESCE(hash, X''), COALESCE(device_id, ''), version, local_version, remote_version, local_mtime, remote_mtime
             FROM file_metadata WHERE folder_id = ?1 AND path = ?2",
        )?;
        let mut rows = stmt.query(rusqlite::params![folder_id.0, path.0])?;
        match rows.next()? {
            Some(row) => Ok(Some(FileMetadata {
                path: FilePath(row.get(0)?),
                folder_id: FolderId(row.get(1)?),
                kind: EntryKind::File,
                mtime: row.get(2)?,
                size: row.get(3)?,
                hash: {
                    let raw: Vec<u8> = row.get(4)?;
                    let mut arr = [0u8; 32];
                    let len = raw.len().min(32);
                    arr[..len].copy_from_slice(&raw[..len]);
                    crate::domain::FileHash(arr)
                },
                device_id: DeviceId(row.get(5)?),
                version: row.get(6)?,
                local_version: row.get(7)?,
                remote_version: row.get(8)?,
                local_mtime: row.get(9)?,
                remote_mtime: row.get(10)?,
            })),
            None => Ok(None),
        }
    }

    async fn upsert_file_metadata(&self, meta: &FileMetadata) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO file_metadata (path, folder_id, mtime, size, hash, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(folder_id, path) DO UPDATE SET
               mtime = excluded.mtime,
               size = excluded.size,
               hash = excluded.hash,
               device_id = excluded.device_id",
            rusqlite::params![
                meta.path.0,
                meta.folder_id.0,
                meta.mtime,
                meta.size,
                &meta.hash.0[..],
                meta.device_id.0
            ],
        )?;
        Ok(())
    }

    // ── History ──

    async fn record_history(&self, entry: &HistoryEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO file_history (folder_id, path, device_id, action, version, mtime, hash, size, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())",
            rusqlite::params![
                entry.folder_id.0,
                entry.path.0,
                entry.device_id.0,
                entry.action,
                entry.version,
                entry.mtime,
                entry.hash,
                entry.size
            ],
        )?;
        Ok(())
    }

    // ── Tombstones ──

    async fn get_tombstones(&self, folder_id: FolderId, since: u64) -> Result<Vec<Tombstone>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT folder_id, path, device_id, deleted_at FROM tombstones WHERE folder_id = ?1 AND deleted_at >= ?2",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![folder_id.0, since as i64], |row| {
                Ok(Tombstone {
                    folder_id: FolderId(row.get(0)?),
                    path: FilePath(row.get(1)?),
                    device_id: DeviceId(row.get(2)?),
                    deleted_at: row.get::<_, i64>(3)? as u64,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    async fn add_tombstone(&self, tomb: &Tombstone) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tombstones (folder_id, path, device_id, deleted_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                tomb.folder_id.0,
                tomb.path.0,
                tomb.device_id.0,
                tomb.deleted_at as i64
            ],
        )?;
        Ok(())
    }

    // ── Sessions ──

    async fn record_session(&self, session: &SessionRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_sessions (ts, direction, peer_device, addr, folder_path, pushed_count, pulled_count, conflicts_count, pushed_bytes, pulled_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                session.ts,
                session.direction,
                session.peer_device,
                session.addr,
                session.folder_path,
                session.pushed_count as i64,
                session.pulled_count as i64,
                session.conflicts_count as i64,
                session.pushed_bytes as i64,
                session.pulled_bytes as i64
            ],
        )?;
        Ok(())
    }

    async fn list_recent_sessions(&self, limit: u32) -> Result<Vec<SessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT ts, direction, peer_device, addr, folder_path, pushed_count, pulled_count, conflicts_count, pushed_bytes, pulled_bytes
             FROM sync_sessions ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            Ok(SessionRecord {
                ts: row.get(0)?,
                direction: row.get(1)?,
                peer_device: row.get(2)?,
                addr: row.get(3)?,
                folder_path: row.get(4)?,
                pushed_count: row.get(5)?,
                pulled_count: row.get(6)?,
                conflicts_count: row.get(7)?,
                pushed_bytes: row.get::<_, i64>(8)? as u64,
                pulled_bytes: row.get::<_, i64>(9)? as u64,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<SessionRecord>>>()
            .map_err(Into::into)
    }

    // ── Bulk operations ──

    async fn clear_all_sync_state(&self) -> Result<(usize, usize)> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM file_history", [])?;
        tx.execute("DELETE FROM file_metadata", [])?;
        let folders = tx.execute("DELETE FROM sync_folders", [])?;
        let devices = tx.execute("DELETE FROM devices", [])?;
        tx.commit()?;
        Ok((folders, devices))
    }

    async fn remove_sync_folders(
        &self,
        local_path: &str,
        device_id: Option<&DeviceId>,
    ) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let deleted = match device_id {
            Some(dev) => conn.execute(
                "DELETE FROM sync_folders WHERE local_path = ?1 AND device_id = ?2",
                rusqlite::params![local_path, dev.0],
            )?,
            None => conn.execute(
                "DELETE FROM sync_folders WHERE local_path = ?1",
                rusqlite::params![local_path],
            )?,
        };
        Ok(deleted)
    }
}

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub path: String,
    pub folder_id: i64,
    pub mtime: i64,
    pub size: i64,
    pub hash: Vec<u8>,
    pub device_id: String,
    pub version: i64,
    pub local_version: i64,
    pub remote_version: i64,
    pub local_mtime: i64,
    pub remote_mtime: i64,
}

/// (id, local_path, device_id, direction, last_sync_at)
pub type SyncFolderRow = (i64, String, String, String, Option<i64>);

/// How many finished sessions to keep in `sync_sessions`.
pub const MAX_SESSION_HISTORY: u32 = 100;

/// One finished sync session (outgoing or incoming).
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub ts: i64,
    pub direction: String,
    pub peer_device: String,
    pub addr: String,
    pub folder_path: String,
    pub pushed_count: usize,
    pub pulled_count: usize,
    pub conflicts_count: usize,
    /// Total bytes sent to the peer during the session.
    pub pushed_bytes: u64,
    /// Total bytes received from the peer during the session.
    pub pulled_bytes: u64,
}

/// One file-history entry to record.
pub struct HistoryRecord<'a> {
    pub folder_id: i64,
    pub path: &'a str,
    pub device_id: &'a str,
    pub action: &'a str,
    pub version: i64,
    pub mtime: i64,
    pub hash: &'a [u8],
    pub size: i64,
}

/// One read-out file-history entry, newest first.
#[derive(Debug, Clone)]
pub struct FileHistoryRow {
    pub path: String,
    pub device_id: Option<String>,
    pub action: String,
    pub size: Option<i64>,
    pub recorded_at: i64,
}

/// Encrypted (or plain) metadata storage backed by SQLite.
#[derive(Debug)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

// The device-removal summary DTO is owned by the persistence contract; the
// sqlite store returns the same type. Keeping exactly one `DeviceCleanup` in
// the crate makes FRB codegen deterministic (it merges same-named types).
pub use crate::persistence::traits::DeviceCleanup;

impl Storage {
    /// Open or create the database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        storage.initialize_schema()?;
        Ok(storage)
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
            ",
        )?;
        // Migration for databases created before last_addr existed.
        let _ = conn.execute("ALTER TABLE devices ADD COLUMN last_addr TEXT", []);
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

    // ── device CRUD ──

    pub fn upsert_device(
        &self,
        id: &str,
        name: &str,
        cert_der: Option<&[u8]>,
        last_addr: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO devices (id, name, cert_der, last_seen, last_addr)
             VALUES (?1, ?2, ?3, unixepoch(), ?4)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               cert_der = COALESCE(excluded.cert_der, devices.cert_der),
               last_seen = unixepoch(),
               last_addr = COALESCE(excluded.last_addr, devices.last_addr)",
            rusqlite::params![id, name, cert_der, last_addr],
        )?;
        Ok(())
    }

    /// Look up a device's display name by its id.
    pub fn get_device_name(&self, id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM devices WHERE id = ?1")?;
        let mut rows = stmt.query_map([id], |row| row.get::<_, String>(0))?;
        Ok(rows.next().transpose()?)
    }

    pub fn device_last_addr(&self, id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT last_addr FROM devices WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => Ok(row.get::<_, Option<String>>(0)?),
            None => Ok(None),
        }
    }

    /// Record the last known address for an existing device row. No-op when
    /// the device is unknown.
    pub fn set_device_last_addr(&self, id: &str, addr: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE devices SET last_addr = ?1 WHERE id = ?2",
            rusqlite::params![addr, id],
        )?;
        Ok(())
    }

    pub fn get_device_cert(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT cert_der FROM devices WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => Ok(row.get::<_, Option<Vec<u8>>>(0)?),
            None => Ok(None),
        }
    }

    /// Look up a device by its TLS certificate fingerprint (blake3 hash of DER).
    /// Returns the device_id if a matching certificate is found.
    pub fn get_device_by_cert_fingerprint(&self, fingerprint: &[u8]) -> Result<Option<String>> {
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
                let hash = blake3::hash(&der);
                if hash.as_bytes() == fingerprint {
                    return Ok(Some(id));
                }
            }
        }
        Ok(None)
    }

    /// Update the TLS certificate for a device. Creates the row if it doesn't exist.
    pub fn set_device_cert(&self, id: &str, cert_der: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO devices (id, name, cert_der, last_seen)
             VALUES (?1, ?1, ?2, unixepoch())
             ON CONFLICT(id) DO UPDATE SET cert_der = ?2",
            rusqlite::params![id, cert_der],
        )?;
        Ok(())
    }

    // ── file metadata ──

    pub fn get_file_metadata(&self, folder_id: i64, path: &str) -> Result<Option<FileMetadata>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT path, folder_id, mtime, size, COALESCE(hash, X''), COALESCE(device_id, ''), version, local_version, remote_version, local_mtime, remote_mtime
             FROM file_metadata WHERE folder_id = ?1 AND path = ?2",
        )?;
        let mut rows = stmt.query(rusqlite::params![folder_id, path])?;
        match rows.next()? {
            Some(row) => Ok(Some(FileMetadata {
                path: row.get(0)?,
                folder_id: row.get(1)?,
                mtime: row.get(2)?,
                size: row.get(3)?,
                hash: row.get(4)?,
                device_id: row.get(5)?,
                version: row.get(6)?,
                local_version: row.get(7)?,
                remote_version: row.get(8)?,
                local_mtime: row.get(9)?,
                remote_mtime: row.get(10)?,
            })),
            None => Ok(None),
        }
    }

    pub fn list_devices(&self) -> Result<Vec<(String, String, Option<i64>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, name, last_seen FROM devices ORDER BY last_seen DESC")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn list_sync_folders(&self) -> Result<Vec<SyncFolderRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, local_path, device_id, direction, last_sync_at FROM sync_folders ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    // ── sync folder CRUD ──

    pub fn add_sync_folder(
        &self,
        local_path: &str,
        device_id: &str,
        direction: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_folders (local_path, device_id, direction)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![local_path, device_id, direction],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Stamp the last successful sync time (unix seconds) for a folder.
    pub fn set_folder_last_sync(&self, folder_id: i64, ts: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_folders SET last_sync_at = ?1 WHERE id = ?2",
            rusqlite::params![ts, folder_id],
        )?;
        Ok(())
    }

    /// Re-point a sync-folder row at a different device. Used to adopt
    /// served-bookkeeping rows (which reference ourselves) when a remote
    /// peer is attached. Caller must ensure the device row exists (FK).
    pub fn set_folder_device(&self, folder_id: i64, new_device_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_folders SET device_id = ?1 WHERE id = ?2",
            rusqlite::params![new_device_id, folder_id],
        )?;
        Ok(())
    }

    /// Record a finished sync session. Keeps only the most recent
    /// [`MAX_SESSION_HISTORY`] rows.
    #[allow(clippy::too_many_arguments)]
    pub fn record_session(
        &self,
        direction: &str,
        peer_device: &str,
        addr: &str,
        folder_path: &str,
        pushed: usize,
        pulled: usize,
        conflicts: usize,
        pushed_bytes: u64,
        pulled_bytes: u64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_sessions
             (ts, direction, peer_device, addr, folder_path,
              pushed_count, pulled_count, conflicts_count, pushed_bytes, pulled_bytes)
             VALUES (unixepoch(), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                direction,
                peer_device,
                addr,
                folder_path,
                pushed as i64,
                pulled as i64,
                conflicts as i64,
                pushed_bytes as i64,
                pulled_bytes as i64
            ],
        )?;
        conn.execute(
            "DELETE FROM sync_sessions WHERE id NOT IN
             (SELECT id FROM sync_sessions ORDER BY id DESC LIMIT ?1)",
            rusqlite::params![MAX_SESSION_HISTORY],
        )?;
        Ok(())
    }

    /// Most recent recorded sessions, newest first.
    pub fn list_recent_sessions(&self, limit: u32) -> Result<Vec<SessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT ts, direction, peer_device, addr, folder_path,
                    pushed_count, pulled_count, conflicts_count, pushed_bytes, pulled_bytes
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

    /// Most recent recorded sessions with a given peer (typically a device id
    /// for outgoing sessions), newest first.
    pub fn list_sessions_for_device(
        &self,
        device_id: &str,
        limit: u32,
    ) -> Result<Vec<SessionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "SELECT ts, direction, peer_device, addr, folder_path,
                    pushed_count, pulled_count, conflicts_count, pushed_bytes, pulled_bytes
             FROM sync_sessions WHERE peer_device = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![device_id, limit as i64], |row| {
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

    /// Remove sync-folder rows by path, optionally narrowed to one device.
    /// Returns how many rows were deleted (a path can map to several rows).
    pub fn remove_sync_folders(&self, local_path: &str, device_id: Option<&str>) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let deleted = match device_id {
            Some(dev) => conn.execute(
                "DELETE FROM sync_folders WHERE local_path = ?1 AND device_id = ?2",
                rusqlite::params![local_path, dev],
            )?,
            None => conn.execute(
                "DELETE FROM sync_folders WHERE local_path = ?1",
                rusqlite::params![local_path],
            )?,
        };
        Ok(deleted)
    }

    /// Remove a single sync folder by its database id.
    pub fn remove_sync_folder_by_id(&self, folder_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM sync_folders WHERE id = ?1",
            rusqlite::params![folder_id],
        )?;
        Ok(())
    }

    /// Wipe every pairing and folder mapping in one transaction: all
    /// `sync_folders` and `devices` rows plus the per-folder caches
    /// (`file_metadata`, `file_history`) so recreated folders start from a
    /// clean index. Returns `(folders_removed, devices_removed)`.
    pub fn clear_all_sync_state(&self) -> Result<(usize, usize)> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        // Children first: the cache tables reference folders/devices.
        tx.execute("DELETE FROM file_history", [])?;
        tx.execute("DELETE FROM file_metadata", [])?;
        let folders = tx.execute("DELETE FROM sync_folders", [])?;
        let devices = tx.execute("DELETE FROM devices", [])?;
        tx.commit()?;
        Ok((folders, devices))
    }

    /// Remove a paired device and all associated data in one transaction:
    /// `sync_sessions` → `file_history` → `file_metadata` → `sync_folders`
    /// → `devices`. Returns a breakdown of deleted row counts.
    pub fn remove_device(&self, device_id: &str) -> Result<DeviceCleanup> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        // Children first to avoid FK issues (pragma is off).
        let sessions = tx.execute(
            "DELETE FROM sync_sessions WHERE peer_device = ?1",
            rusqlite::params![device_id],
        )?;
        let history = tx.execute(
            "DELETE FROM file_history WHERE device_id = ?1 \
             OR folder_id IN (SELECT id FROM sync_folders WHERE device_id = ?1)",
            rusqlite::params![device_id],
        )?;
        let metadata = tx.execute(
            "DELETE FROM file_metadata \
             WHERE folder_id IN (SELECT id FROM sync_folders WHERE device_id = ?1)",
            rusqlite::params![device_id],
        )?;
        let folders = tx.execute(
            "DELETE FROM sync_folders WHERE device_id = ?1",
            rusqlite::params![device_id],
        )?;
        let device = tx.execute(
            "DELETE FROM devices WHERE id = ?1",
            rusqlite::params![device_id],
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

    pub fn upsert_file_metadata(
        &self,
        folder_id: i64,
        path: &str,
        mtime: i64,
        size: i64,
        hash: &[u8],
        device_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO file_metadata (path, folder_id, mtime, size, hash, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(folder_id, path) DO UPDATE SET
               mtime = excluded.mtime,
               size = excluded.size,
               hash = excluded.hash,
               device_id = excluded.device_id",
            rusqlite::params![path, folder_id, mtime, size, hash, device_id],
        )?;
        Ok(())
    }

    // ── file history ──

    pub fn record_history(&self, rec: HistoryRecord<'_>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO file_history (folder_id, path, device_id, action, version, mtime, hash, size, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())",
            rusqlite::params![
                rec.folder_id,
                rec.path,
                rec.device_id,
                rec.action,
                rec.version,
                rec.mtime,
                rec.hash,
                rec.size
            ],
        )?;
        Ok(())
    }

    /// Most recent file-history entries for a folder (or across all folders
    /// when `folder_id` is `None`), newest first.
    pub fn list_file_history(
        &self,
        folder_id: Option<i64>,
        limit: u32,
    ) -> Result<Vec<FileHistoryRow>> {
        let conn = self.conn.lock().unwrap();
        let (sql, params): (&str, Vec<rusqlite::types::Value>) = match folder_id {
            Some(id) => (
                "SELECT path, device_id, action, size, recorded_at
                 FROM file_history WHERE folder_id = ?1 ORDER BY id DESC LIMIT ?2",
                vec![
                    rusqlite::types::Value::Integer(id),
                    rusqlite::types::Value::Integer(limit as i64),
                ],
            ),
            None => (
                "SELECT path, device_id, action, size, recorded_at
                 FROM file_history ORDER BY id DESC LIMIT ?1",
                vec![rusqlite::types::Value::Integer(limit as i64)],
            ),
        };
        let mut stmt = conn.prepare_cached(sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            Ok(FileHistoryRow {
                path: row.get(0)?,
                device_id: row.get(1)?,
                action: row.get(2)?,
                size: row.get(3)?,
                recorded_at: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<FileHistoryRow>>>()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn in_memory() -> Storage {
        Storage::open(&PathBuf::from(":memory:")).unwrap()
    }

    #[test]
    fn set_device_cert_creates_row_when_missing() {
        let s = in_memory();
        // No device row exists yet — old UPDATE-only logic would silently no-op.
        s.set_device_cert("dev-1", &[0x01, 0x02, 0x03]).unwrap();
        let cert = s.get_device_cert("dev-1").unwrap().expect("cert should exist");
        assert_eq!(cert, [0x01, 0x02, 0x03]);
    }

    #[test]
    fn set_device_cert_updates_existing_row() {
        let s = in_memory();
        s.upsert_device("dev-1", "Laptop", None, None).unwrap();
        s.set_device_cert("dev-1", &[0xAA]).unwrap();
        let cert = s.get_device_cert("dev-1").unwrap().expect("cert should exist");
        assert_eq!(cert, [0xAA]);
    }

    #[test]
    fn upsert_device_does_not_overwrite_existing_cert_with_none() {
        let s = in_memory();
        s.set_device_cert("dev-1", &[0x01]).unwrap();
        // Second upsert with cert_der=None should preserve the existing cert.
        s.upsert_device("dev-1", "Laptop", None, None).unwrap();
        let cert = s.get_device_cert("dev-1").unwrap().expect("cert should exist");
        assert_eq!(cert, [0x01]);
    }
}

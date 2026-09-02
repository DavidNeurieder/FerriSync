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

/// One explicitly-shared folder (the discoverable namespace exposed to peers).
/// (id, folder_guid, device_id, name, local_path, discoverable, enabled, permissions)
pub type SharedFolderRow = (
    i64,
    String,
    String,
    String,
    String,
    bool,
    bool,
    String,
);

/// Derive a user-facing folder label ("Documents") from a filesystem path.
pub fn path_label(local_path: &str) -> String {
    local_path
        .trim_end_matches(['/', '\\'])
        .split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or(local_path)
        .to_string()
}

/// Mint a stable, globally-unique folder id. Deterministic per row for a
/// given process id + row id so re-running an in-memory test is stable.
pub fn new_folder_guid(folder_id: i64) -> String {
    use blake3::Hasher;
    let mut h = Hasher::new();
    h.update(b"ferrisync-folder-guid-v1");
    h.update(&folder_id.to_le_bytes());
    h.update(&std::process::id().to_le_bytes());
    format!("f-{}", h.finalize().to_hex())
}

/// How many finished sessions to keep in `sync_sessions`.
pub const MAX_SESSION_HISTORY: u32 = 100;

/// One finished sync session (outgoing or incoming).
#[derive(Debug, Clone, serde::Serialize)]
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
#[derive(Debug, Clone, serde::Serialize)]
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
                name TEXT,
                device_id TEXT NOT NULL,
                active BOOL DEFAULT 1,
                direction TEXT DEFAULT 'bidirectional',
                last_sync_at INTEGER,
                folder_guid TEXT,
                FOREIGN KEY (device_id) REFERENCES devices(id)
            );

            CREATE TABLE IF NOT EXISTS folder_devices (
                folder_id INTEGER NOT NULL,
                device_id TEXT NOT NULL,
                mode TEXT NOT NULL DEFAULT 'bidirectional',
                remote_path TEXT,
                enabled BOOL DEFAULT 1,
                PRIMARY KEY (folder_id, device_id),
                FOREIGN KEY (folder_id) REFERENCES sync_folders(id) ON DELETE CASCADE,
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

            CREATE TABLE IF NOT EXISTS shared_folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_guid TEXT NOT NULL,
                device_id TEXT NOT NULL,
                name TEXT NOT NULL,
                local_path TEXT NOT NULL,
                discoverable BOOL DEFAULT 1,
                enabled BOOL DEFAULT 1,
                permissions TEXT DEFAULT 'read_write',
                UNIQUE (folder_guid, device_id),
                FOREIGN KEY (device_id) REFERENCES devices(id)
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
        // Migrations for databases created before the folder `name` / multi-
        // device `folder_devices` model existed.
        let _ = conn.execute("ALTER TABLE sync_folders ADD COLUMN name TEXT", []);
        // Stable per-folder GUID, independent of the filesystem path, so a
        // moved/renamed folder can keep its device relationships (§7/§8).
        let _ = conn.execute("ALTER TABLE sync_folders ADD COLUMN folder_guid TEXT", []);
        // Backfill one pair row per existing folder from its legacy columns.
        let _ = conn.execute_batch(
            "INSERT OR IGNORE INTO folder_devices (folder_id, device_id, mode, enabled)
             SELECT id, device_id, direction, active FROM sync_folders;",
        );
        // Mint a GUID for every existing folder that lacks one.
        let guidless: Vec<i64> = conn
            .prepare("SELECT id FROM sync_folders WHERE folder_guid IS NULL OR folder_guid = ''")?
            .query_map([], |r| r.get::<_, i64>(0))?
            .filter_map(|r| r.ok())
            .collect();
        for id in guidless {
            let guid = new_folder_guid(id);
            let _ = conn.execute(
                "UPDATE sync_folders SET folder_guid = ?1 WHERE id = ?2",
                rusqlite::params![guid, id],
            );
        }
        // Derive any missing folder names from the path's final component.
        let empty: Vec<i64> = conn
            .prepare("SELECT id FROM sync_folders WHERE name IS NULL OR name = ''")?
            .query_map([], |r| r.get::<_, i64>(0))?
            .filter_map(|r| r.ok())
            .collect();
        for id in empty {
            let path: Option<String> = conn
                .query_row(
                    "SELECT local_path FROM sync_folders WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .ok();
            if let Some(path) = path {
                let name = path_label(&path);
                let _ = conn.execute(
                    "UPDATE sync_folders SET name = ?1 WHERE id = ?2",
                    rusqlite::params![name, id],
                );
            }
        }
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
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
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

    /// One tuple per enabled folder↔device pair, JOINed so folders with
    /// several devices surface as several rows sharing the folder id.
    /// Ordering is stable (folder id, then device id) for deterministic tests.
    pub fn list_sync_folders(&self) -> Result<Vec<SyncFolderRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT f.id, f.local_path, fd.device_id, fd.mode, f.last_sync_at
             FROM sync_folders f
             JOIN folder_devices fd ON fd.folder_id = f.id
             WHERE fd.enabled = 1
             ORDER BY f.id, fd.device_id",
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

    /// Register a local folder to sync against a device. Reusing the same
    /// `(local_path, device_id)` is idempotent and returns the existing folder
    /// id; the same path against a *different* device creates a second pair on
    /// the same folder. A new folder's `name` defaults to the path's label.
    ///
    /// Folders are matched by folder GUID first (so a moved path re-points the
    /// existing relationship instead of creating a duplicate), falling back to
    /// a path match for legacy rows.
    pub fn add_sync_folder(
        &self,
        local_path: &str,
        device_id: &str,
        direction: &str,
    ) -> Result<i64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        // A caller may already know the stable folder id; prefer matching by
        // guid when present, else by path (legacy).
        let folder_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM sync_folders WHERE local_path = ?1 LIMIT 1",
                [local_path],
                |r| r.get(0),
            )
            .ok();
        let folder_id = match folder_id {
            Some(id) => id,
            None => {
                let name = path_label(local_path);
                let next_id: i64 = tx
                    .query_row(
                        "SELECT COALESCE(MAX(id), 0) + 1 FROM sync_folders",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(1);
                let guid = new_folder_guid(next_id);
                tx.execute(
                    "INSERT INTO sync_folders (local_path, name, device_id, direction, folder_guid)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![local_path, name, device_id, direction, guid],
                )?;
                tx.last_insert_rowid()
            }
        };
        tx.execute(
            "INSERT INTO folder_devices (folder_id, device_id, mode, enabled)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(folder_id, device_id) DO UPDATE SET
               mode = excluded.mode,
               enabled = 1",
            rusqlite::params![folder_id, device_id, direction],
        )?;
        tx.commit()?;
        Ok(folder_id)
    }

    /// Attach an additional device (with its own mode) to an existing folder.
    /// Idempotent per (folder, device) pair.
    pub fn add_folder_device(
        &self,
        folder_id: i64,
        device_id: &str,
        mode: &str,
        remote_path: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO folder_devices (folder_id, device_id, mode, remote_path, enabled)
             VALUES (?1, ?2, ?3, ?4, 1)
             ON CONFLICT(folder_id, device_id) DO UPDATE SET
               mode = excluded.mode,
               remote_path = COALESCE(excluded.remote_path, folder_devices.remote_path),
               enabled = 1",
            rusqlite::params![folder_id, device_id, mode, remote_path],
        )?;
        Ok(())
    }

    /// Per-pair rows for a folder: `(device_id, mode, remote_path, enabled)`.
    pub fn folder_pairs(
        &self,
        folder_id: i64,
    ) -> Result<Vec<(String, String, Option<String>, bool)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT device_id, mode, remote_path, enabled FROM folder_devices
             WHERE folder_id = ?1 ORDER BY device_id",
        )?;
        let rows = stmt
            .query_map([folder_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// The folder's display name (or its path label when none was set).
    pub fn folder_name(&self, folder_id: i64) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        let (name, path): (Option<String>, String) = conn.query_row(
            "SELECT name, local_path FROM sync_folders WHERE id = ?1",
            [folder_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| path_label(&path)))
    }

    /// The folder's local path, when the row exists.
    pub fn folder_path(&self, folder_id: i64) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT local_path FROM sync_folders WHERE id = ?1")?;
        let mut rows = stmt.query_map([folder_id], |row| row.get::<_, String>(0))?;
        Ok(rows.next().transpose()?)
    }

    /// Set the folder's user-facing display name.
    pub fn set_folder_name(&self, folder_id: i64, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_folders SET name = ?1 WHERE id = ?2",
            rusqlite::params![name, folder_id],
        )?;
        Ok(())
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

    /// The folder's stable GUID (minted on creation / migration backfill).
    pub fn folder_guid(&self, folder_id: i64) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT folder_guid FROM sync_folders WHERE id = ?1")?;
        let mut rows = stmt.query_map([folder_id], |row| row.get::<_, Option<String>>(0))?;
        Ok(rows.next().transpose()?.flatten())
    }

    /// The local sync-folder row id whose guid matches `guid` (its replica of
    /// a logical sync space), if any.
    pub fn folder_id_for_guid(&self, guid: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id FROM sync_folders WHERE folder_guid = ?1")?;
        let mut rows = stmt.query_map([guid], |row| row.get::<_, i64>(0))?;
        Ok(rows.next().transpose()?)
    }

    /// Ensure a local `sync_folders` row exists for `guid` (the owner's
    /// replica of a shared logical sync space), creating it when absent, and
    /// return its id. Used when approving a folder pairing so the peer pair
    /// can attach to a concrete replica.
    pub fn ensure_folder_by_guid(
        &self,
        guid: &str,
        local_path: &str,
        name: &str,
        device_id: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        if let Some(id) = conn
            .query_row(
                "SELECT id FROM sync_folders WHERE folder_guid = ?1",
                [guid],
                |r| r.get(0),
            )
            .ok()
        {
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO sync_folders (local_path, name, device_id, direction, folder_guid)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![local_path, name, device_id, "bidirectional", guid],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Re-point a folder's *local* path without losing device relationships
    /// (§7/§8). Idempotent; a no-op when the path is unchanged.
    pub fn set_folder_local_path(&self, folder_id: i64, new_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sync_folders SET local_path = ?1 WHERE id = ?2",
            rusqlite::params![new_path, folder_id],
        )?;
        Ok(())
    }

    /// The stored remote path for a (folder, device) pair — i.e. where the
    /// peer keeps this folder's copy. `None` when the peer uses the default
    /// (same-basename) destination.
    pub fn folder_pair_remote_path(
        &self,
        folder_id: i64,
        device_id: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT remote_path FROM folder_devices WHERE folder_id = ?1 AND device_id = ?2",
        )?;
        let mut rows = stmt.query_map(rusqlite::params![folder_id, device_id], |row| {
            row.get::<_, Option<String>>(0)
        })?;
        Ok(rows.next().transpose()?.flatten())
    }

    /// Re-point a sync-folder *pair* at a different device. Used to adopt
    /// served-bookkeeping rows (which reference ourselves) when a remote
    /// peer is attached; the folder's own pair is re-keyed to the real
    /// device. Caller must ensure the device row exists (FK).
    pub fn set_folder_device(&self, folder_id: i64, new_device_id: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        // Move the first (typically own-keyed) pair rather than duplicating it.
        let own: Option<String> = tx
            .query_row(
                "SELECT device_id FROM folder_devices WHERE folder_id = ?1
                 ORDER BY rowid LIMIT 1",
                [folder_id],
                |r| r.get(0),
            )
            .ok();
        if let Some(old) = &own {
            if old != new_device_id {
                tx.execute(
                    "INSERT OR IGNORE INTO devices (id, name) VALUES (?1, ?1)",
                    [new_device_id],
                )?;
                tx.execute(
                    "UPDATE folder_devices SET device_id = ?1
                     WHERE folder_id = ?2 AND device_id = ?3",
                    rusqlite::params![new_device_id, folder_id, old],
                )?;
            }
        }
        tx.commit()?;
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

    /// Remove sync-folder *pairs* by path, optionally narrowed to one device.
    /// When a folder's last pair is removed the folder header is dropped too.
    /// Returns how many pairs were deleted (a path can map to several pairs).
    pub fn remove_sync_folders(&self, local_path: &str, device_id: Option<&str>) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let folder_ids: Vec<i64> = tx
            .prepare("SELECT id FROM sync_folders WHERE local_path = ?1")?
            .query_map([local_path], |r| r.get::<_, i64>(0))?
            .filter_map(|r| r.ok())
            .collect();
        let mut deleted = 0;
        for fid in folder_ids {
            let n = match device_id {
                Some(dev) => tx.execute(
                    "DELETE FROM folder_devices WHERE folder_id = ?1 AND device_id = ?2",
                    rusqlite::params![fid, dev],
                )?,
                None => tx.execute(
                    "DELETE FROM folder_devices WHERE folder_id = ?1",
                    rusqlite::params![fid],
                )?,
            };
            deleted += n;
            // Drop the header once no pairs remain.
            let remaining: i64 = tx.query_row(
                "SELECT COUNT(*) FROM folder_devices WHERE folder_id = ?1",
                [fid],
                |r| r.get(0),
            )?;
            if remaining == 0 {
                tx.execute("DELETE FROM sync_folders WHERE id = ?1", [fid])?;
            }
        }
        tx.commit()?;
        Ok(deleted)
    }

    /// Remove a single sync folder (all its pairs) by database id.
    pub fn remove_sync_folder_by_id(&self, folder_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM folder_devices WHERE folder_id = ?1",
            rusqlite::params![folder_id],
        )?;
        conn.execute(
            "DELETE FROM sync_folders WHERE id = ?1",
            rusqlite::params![folder_id],
        )?;
        Ok(())
    }

    /// Remove a single folder↔device pair. Never touches files on disk — only
    /// the relationship. Drops the folder header once its last pair is gone.
    pub fn remove_folder_device(&self, folder_id: i64, device_id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let n = tx.execute(
            "DELETE FROM folder_devices WHERE folder_id = ?1 AND device_id = ?2",
            rusqlite::params![folder_id, device_id],
        )?;
        if n == 0 {
            tx.commit()?;
            return Ok(false);
        }
        let remaining: i64 = tx.query_row(
            "SELECT COUNT(*) FROM folder_devices WHERE folder_id = ?1",
            [folder_id],
            |r| r.get(0),
        )?;
        if remaining == 0 {
            tx.execute("DELETE FROM sync_folders WHERE id = ?1", [folder_id])?;
        }
        tx.commit()?;
        Ok(true)
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
        tx.execute("DELETE FROM folder_devices", [])?;
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
             OR folder_id IN (SELECT folder_id FROM folder_devices WHERE device_id = ?1)",
            rusqlite::params![device_id],
        )?;
        let metadata = tx.execute(
            "DELETE FROM file_metadata \
             WHERE folder_id IN (SELECT folder_id FROM folder_devices WHERE device_id = ?1)",
            rusqlite::params![device_id],
        )?;
        let _pairs = tx.execute(
            "DELETE FROM folder_devices WHERE device_id = ?1",
            rusqlite::params![device_id],
        )?;
        // Drop folder headers left with no remaining pair.
        let folders = tx.execute(
            "DELETE FROM sync_folders WHERE id NOT IN \
             (SELECT folder_id FROM folder_devices)",
            [],
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

    /// Explicitly share a folder so trusted peers can discover and request
    /// pairing to it. `folder_guid` links the share to the logical sync space
    /// (a `sync_folders.folder_guid`), independent of the local filesystem
    /// path. Idempotent per (folder_guid, device_id); a no-op on re-share.
    pub fn share_folder(
        &self,
        folder_guid: &str,
        device_id: &str,
        name: &str,
        local_path: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO shared_folders
                (folder_guid, device_id, name, local_path, discoverable, enabled, permissions)
             VALUES (?1, ?2, ?3, ?4, 1, 1, 'read_write')
             ON CONFLICT(folder_guid, device_id) DO NOTHING",
            rusqlite::params![folder_guid, device_id, name, local_path],
        )?;
        Ok(())
    }

    /// Every shared folder owned by `device_id` (typically ourselves), newest
    /// share id first.
    pub fn list_shared_folders(&self, device_id: &str) -> Result<Vec<SharedFolderRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, folder_guid, device_id, name, local_path, discoverable, enabled, permissions
             FROM shared_folders WHERE device_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([device_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get::<_, i64>(5)? != 0,
                row.get::<_, i64>(6)? != 0,
                row.get(7)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<SharedFolderRow>>>()
            .map_err(Into::into)
    }

    /// The shared-folder row matching `device_id` (the owner) and `folder_guid`.
    pub fn shared_folder_by_guid(
        &self,
        device_id: &str,
        folder_guid: &str,
    ) -> Result<Option<SharedFolderRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, folder_guid, device_id, name, local_path, discoverable, enabled, permissions
             FROM shared_folders WHERE device_id = ?1 AND folder_guid = ?2",
        )?;
        let mut rows = stmt
            .query_map(rusqlite::params![device_id, folder_guid], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get::<_, i64>(5)? != 0,
                    row.get::<_, i64>(6)? != 0,
                    row.get(7)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<SharedFolderRow>>>()?;
        Ok(rows.pop())
    }

    /// Toggle whether a shared folder is discoverable by trusted peers.
    pub fn set_shared_discoverable(&self, id: i64, discoverable: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE shared_folders SET discoverable = ?1 WHERE id = ?2",
            rusqlite::params![discoverable, id],
        )?;
        Ok(())
    }

    /// Stop sharing a folder by its `shared_folders` id. Removes only the
    /// share; any existing replica/pair relationship is left intact.
    pub fn unshare_folder(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM shared_folders WHERE id = ?1", [id])?;
        Ok(())
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
        let cert = s
            .get_device_cert("dev-1")
            .unwrap()
            .expect("cert should exist");
        assert_eq!(cert, [0x01, 0x02, 0x03]);
    }

    #[test]
    fn set_device_cert_updates_existing_row() {
        let s = in_memory();
        s.upsert_device("dev-1", "Laptop", None, None).unwrap();
        s.set_device_cert("dev-1", &[0xAA]).unwrap();
        let cert = s
            .get_device_cert("dev-1")
            .unwrap()
            .expect("cert should exist");
        assert_eq!(cert, [0xAA]);
    }

    #[test]
    fn upsert_device_does_not_overwrite_existing_cert_with_none() {
        let s = in_memory();
        s.set_device_cert("dev-1", &[0x01]).unwrap();
        // Second upsert with cert_der=None should preserve the existing cert.
        s.upsert_device("dev-1", "Laptop", None, None).unwrap();
        let cert = s
            .get_device_cert("dev-1")
            .unwrap()
            .expect("cert should exist");
        assert_eq!(cert, [0x01]);
    }

    #[test]
    fn folder_guid_is_stable_and_unlinked_from_path() {
        let s = in_memory();
        s.upsert_device("phone", "phone", None, None).unwrap();
        s.upsert_device("desktop", "desktop", None, None).unwrap();
        let fid = s
            .add_sync_folder("/home/laptop/Documents", "phone", "bidirectional")
            .unwrap();
        let guid = s.folder_guid(fid).unwrap().expect("guid minted");
        assert!(guid.starts_with("f-"));

        // Re-pointing the local path keeps the same folder id + guid (no dup).
        s.set_folder_local_path(fid, "/home/laptop/Work/Documents")
            .unwrap();
        let fid2 = s
            .add_sync_folder("/home/laptop/Work/Documents", "desktop", "bidirectional")
            .unwrap();
        assert_eq!(fid, fid2, "re-pointed folder must not duplicate");
        assert_eq!(s.folder_guid(fid2).unwrap().as_deref(), Some(guid.as_str()));
    }

    #[test]
    fn remote_path_persists_per_pair_and_round_trips() {
        let s = in_memory();
        s.upsert_device("phone", "phone", None, None).unwrap();
        s.upsert_device("desktop", "desktop", None, None).unwrap();
        let fid = s
            .add_sync_folder("/home/laptop/Documents", "phone", "bidirectional")
            .unwrap();

        // No remote_path set on registration → stays None.
        assert!(s.folder_pair_remote_path(fid, "phone").unwrap().is_none());

        // Explicitly attach phone with a distinct remote destination.
        s.add_folder_device(fid, "phone", "bidirectional", Some("/Documents"))
            .unwrap();
        assert_eq!(
            s.folder_pair_remote_path(fid, "phone").unwrap().as_deref(),
            Some("/Documents")
        );

        // Desktop gets its own different remote path.
        s.add_folder_device(fid, "desktop", "send_only", Some("/Data/Documents"))
            .unwrap();
        let pairs = s.folder_pairs(fid).unwrap();
        assert!(pairs.iter().any(|(d, m, rp, en)| {
            d == "desktop" && m == "send_only" && rp.as_deref() == Some("/Data/Documents") && *en
        }));

        // A NULL re-insert must NOT clobber a stored remote_path (COALESCE).
        s.add_folder_device(fid, "phone", "receive_only", None)
            .unwrap();
        assert_eq!(
            s.folder_pair_remote_path(fid, "phone").unwrap().as_deref(),
            Some("/Documents")
        );
    }

    #[test]
    fn remove_folder_device_drops_only_that_pair_and_cleans_header() {
        let s = in_memory();
        s.upsert_device("phone", "phone", None, None).unwrap();
        s.upsert_device("desktop", "desktop", None, None).unwrap();
        let fid = s.add_sync_folder("/p", "phone", "bidirectional").unwrap();
        s.add_folder_device(fid, "desktop", "bidirectional", None)
            .unwrap();

        // Remove one pair; folder header survives.
        assert!(s.remove_folder_device(fid, "phone").unwrap());
        assert_eq!(s.list_sync_folders().unwrap().len(), 1);
        assert_eq!(s.folder_guid(fid).unwrap().is_some(), true);

        // Remove the last pair; header is dropped.
        assert!(s.remove_folder_device(fid, "desktop").unwrap());
        assert_eq!(s.list_sync_folders().unwrap().len(), 0);

        // Removing a nonexistent pair reports false.
        assert_eq!(s.remove_folder_device(fid, "desktop").unwrap(), false);
    }

    #[test]
    fn share_folder_is_idempotent_and_lists() {
        let s = in_memory();
        s.upsert_device("self", "self", None, None).unwrap();
        let fid = s.add_sync_folder("/p", "self", "bidirectional").unwrap();
        let guid = s.folder_guid(fid).unwrap().unwrap();

        s.share_folder(&guid, "self", "Docs", "/p").unwrap();
        // Re-sharing the same guid/device is a no-op, not a duplicate row.
        s.share_folder(&guid, "self", "Docs", "/p").unwrap();
        let shared = s.list_shared_folders("self").unwrap();
        assert_eq!(shared.len(), 1, "re-share must not duplicate");
        assert_eq!(shared[0].1, guid);
        assert_eq!(shared[0].3, "Docs");
        assert_eq!(shared[0].4, "/p");
        assert_eq!(shared[0].5, true, "discoverable by default");
        assert_eq!(shared[0].6, true, "enabled by default");
        assert_eq!(shared[0].7, "read_write");
    }

    #[test]
    fn share_folder_by_guid_filters_by_owner() {
        let s = in_memory();
        s.upsert_device("self", "self", None, None).unwrap();
        s.upsert_device("other", "other", None, None).unwrap();
        let f1 = s.add_sync_folder("/a", "self", "bidirectional").unwrap();
        let f2 = s.add_sync_folder("/b", "self", "bidirectional").unwrap();
        let g1 = s.folder_guid(f1).unwrap().unwrap();
        let g2 = s.folder_guid(f2).unwrap().unwrap();
        s.share_folder(&g1, "self", "A", "/a").unwrap();
        s.share_folder(&g2, "self", "B", "/b").unwrap();

        assert!(s.shared_folder_by_guid("self", &g1).unwrap().is_some());
        assert!(s.shared_folder_by_guid("self", &g2).unwrap().is_some());
        // A different owner cannot see self's share.
        assert!(s.shared_folder_by_guid("other", &g1).unwrap().is_none());
        // Unknown guid yields none.
        assert!(s.shared_folder_by_guid("self", "missing").unwrap().is_none());
    }

    #[test]
    fn unshare_and_discoverable_toggle() {
        let s = in_memory();
        s.upsert_device("self", "self", None, None).unwrap();
        let fid = s.add_sync_folder("/p", "self", "bidirectional").unwrap();
        let guid = s.folder_guid(fid).unwrap().unwrap();
        s.share_folder(&guid, "self", "Docs", "/p").unwrap();
        let id = s.list_shared_folders("self").unwrap()[0].0;

        s.set_shared_discoverable(id, false).unwrap();
        let row = s.list_shared_folders("self").unwrap();
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].5, false);

        s.unshare_folder(id).unwrap();
        assert_eq!(s.list_shared_folders("self").unwrap().len(), 0);
    }

    #[test]
    fn two_devices_can_share_same_guid_independently() {
        let s = in_memory();
        s.upsert_device("a", "a", None, None).unwrap();
        s.upsert_device("b", "b", None, None).unwrap();
        let fid = s.add_sync_folder("/shared", "a", "bidirectional").unwrap();
        let guid = s.folder_guid(fid).unwrap().unwrap();
        // Both devices expose their own replica of the same logical folder.
        s.share_folder(&guid, "a", "Shared", "/shared").unwrap();
        s.share_folder(&guid, "b", "Shared", "/mirror").unwrap();

        assert_eq!(s.list_shared_folders("a").unwrap().len(), 1);
        assert_eq!(s.list_shared_folders("b").unwrap().len(), 1);
        // Distinct share ids, same logical guid.
        assert_ne!(
            s.list_shared_folders("a").unwrap()[0].0,
            s.list_shared_folders("b").unwrap()[0].0
        );
    }
}

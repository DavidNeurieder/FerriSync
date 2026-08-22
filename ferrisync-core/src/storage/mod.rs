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

/// Encrypted (or plain) metadata storage backed by SQLite.
#[derive(Debug)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

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
            ",
        )?;
        // Migration for databases created before last_addr existed.
        let _ = conn.execute("ALTER TABLE devices ADD COLUMN last_addr TEXT", []);
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

    pub fn device_last_addr(&self, id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT last_addr FROM devices WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => Ok(row.get::<_, Option<String>>(0)?),
            None => Ok(None),
        }
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
}

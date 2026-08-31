use rusqlite::{params, Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub initial_path: String,
    pub key_path: Option<String>,
    #[serde(default)]
    pub key_passphrase_not_required: bool,
    pub host_key: Option<String>,
    #[serde(default)]
    pub local_directory: Option<String>,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub s3_region: Option<String>,
    #[serde(default)]
    pub s3_endpoint: Option<String>,
    #[serde(default)]
    pub s3_force_path_style: bool,
    #[serde(default)]
    pub s3_preserve_empty_directories: bool,
    #[serde(default)]
    pub smb_share: Option<String>,
    #[serde(default)]
    pub smb_domain: Option<String>,
    #[serde(default)]
    pub smb_guest: bool,
    #[serde(default)]
    pub google_drive_location_kind: Option<String>,
    #[serde(default)]
    pub google_drive_location_id: Option<String>,
    #[serde(default)]
    pub transfer_max_concurrent: Option<u32>,
    #[serde(default)]
    pub transfer_bandwidth_limit_kbps: Option<u64>,
    #[serde(default)]
    pub transfer_retry_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionHistory {
    pub bookmark_id: String,
    pub name: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub connected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferHistory {
    pub id: String,
    pub name: String,
    pub direction: String,
    pub status: String,
    pub detail: String,
    pub bytes: u64,
    #[serde(default)]
    pub completed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferJob {
    pub id: String,
    pub connection_id: String,
    pub name: String,
    pub direction: String,
    pub local_path: String,
    pub remote_path: String,
    pub status: String,
    pub detail: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub retry_count: u32,
    pub conflict_policy: String,
    pub is_directory: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHistory {
    pub id: String,
    pub direction: String,
    pub local_directory: String,
    pub remote_directory: String,
    pub status: String,
    pub completed_items: u64,
    pub total_items: u64,
    pub bytes: u64,
    pub detail: String,
    #[serde(default)]
    pub completed_at: String,
}

#[derive(Clone)]
pub struct BookmarkStore {
    database_path: PathBuf,
}

impl BookmarkStore {
    pub fn new(data_directory: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(data_directory)
            .map_err(|error| format!("Failed to create app data directory: {error}"))?;
        let store = Self { database_path: data_directory.join("harbor-transfer.sqlite3") };
        store.with_connection(|connection| {
            connection.pragma_update(None, "journal_mode", "WAL")?;
            connection.pragma_update(None, "synchronous", "NORMAL")?;
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS bookmarks (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    protocol TEXT NOT NULL,
                    host TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    username TEXT NOT NULL,
                    initial_path TEXT NOT NULL DEFAULT '/',
                    key_path TEXT,
                    key_passphrase_not_required INTEGER NOT NULL DEFAULT 0,
                    host_key TEXT,
                    local_directory TEXT,
                    tags TEXT NOT NULL DEFAULT '',
                    s3_region TEXT,
                    s3_endpoint TEXT,
                    s3_force_path_style INTEGER NOT NULL DEFAULT 0,
                    s3_preserve_empty_directories INTEGER NOT NULL DEFAULT 0,
                    smb_share TEXT,
                    smb_domain TEXT,
                    smb_guest INTEGER NOT NULL DEFAULT 0,
                    google_drive_location_kind TEXT,
                    google_drive_location_id TEXT,
                    sort_order INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE IF NOT EXISTS connection_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    bookmark_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    protocol TEXT NOT NULL,
                    host TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    username TEXT NOT NULL,
                    connected_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE IF NOT EXISTS transfer_history (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    direction TEXT NOT NULL,
                    status TEXT NOT NULL,
                    detail TEXT NOT NULL DEFAULT '',
                    bytes INTEGER NOT NULL DEFAULT 0,
                    completed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE IF NOT EXISTS transfer_jobs (
                    id TEXT PRIMARY KEY NOT NULL,
                    connection_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    direction TEXT NOT NULL,
                    local_path TEXT NOT NULL,
                    remote_path TEXT NOT NULL,
                    status TEXT NOT NULL,
                    detail TEXT NOT NULL DEFAULT '',
                    transferred_bytes INTEGER NOT NULL DEFAULT 0,
                    total_bytes INTEGER NOT NULL DEFAULT 0,
                    retry_count INTEGER NOT NULL DEFAULT 0,
                    conflict_policy TEXT NOT NULL DEFAULT 'ask',
                    is_directory INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE IF NOT EXISTS s3_multipart_uploads (
                    transfer_id TEXT PRIMARY KEY NOT NULL,
                    state_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY(transfer_id) REFERENCES transfer_jobs(id) ON DELETE CASCADE
                );
                CREATE TABLE IF NOT EXISTS sync_history (
                    id TEXT PRIMARY KEY NOT NULL,
                    direction TEXT NOT NULL,
                    local_directory TEXT NOT NULL,
                    remote_directory TEXT NOT NULL,
                    status TEXT NOT NULL,
                    completed_items INTEGER NOT NULL DEFAULT 0,
                    total_items INTEGER NOT NULL DEFAULT 0,
                    bytes INTEGER NOT NULL DEFAULT 0,
                    detail TEXT NOT NULL DEFAULT '',
                    completed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );",
            )?;
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN host_key TEXT", []);
            let _ = connection.execute(
                "ALTER TABLE bookmarks ADD COLUMN key_passphrase_not_required INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN local_directory TEXT", []);
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN tags TEXT NOT NULL DEFAULT ''", []);
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN s3_region TEXT", []);
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN s3_endpoint TEXT", []);
            let _ = connection.execute(
                "ALTER TABLE bookmarks ADD COLUMN s3_force_path_style INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = connection.execute(
                "ALTER TABLE bookmarks ADD COLUMN s3_preserve_empty_directories INTEGER NOT NULL DEFAULT 0",
                [],
            );
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN smb_share TEXT", []);
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN smb_domain TEXT", []);
            let _ = connection
                .execute("ALTER TABLE bookmarks ADD COLUMN smb_guest INTEGER NOT NULL DEFAULT 0", []);
            let _ =
                connection.execute("ALTER TABLE bookmarks ADD COLUMN google_drive_location_kind TEXT", []);
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN google_drive_location_id TEXT", []);
            let _ =
                connection.execute("ALTER TABLE bookmarks ADD COLUMN transfer_max_concurrent INTEGER", []);
            let _ = connection
                .execute("ALTER TABLE bookmarks ADD COLUMN transfer_bandwidth_limit_kbps INTEGER", []);
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN transfer_retry_count INTEGER", []);
            let has_sort_order = connection.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('bookmarks') WHERE name = 'sort_order'",
                [],
                |row| row.get::<_, i64>(0),
            )? > 0;
            if !has_sort_order {
                connection.execute("ALTER TABLE bookmarks ADD COLUMN sort_order INTEGER", [])?;
            }
            let missing_sort_order = connection.query_row(
                "SELECT COUNT(*) FROM bookmarks WHERE sort_order IS NULL",
                [],
                |row| row.get::<_, i64>(0),
            )? > 0;
            if missing_sort_order {
                connection.execute_batch(
                    "WITH ordered AS (
                        SELECT id, ROW_NUMBER() OVER (
                            ORDER BY updated_at DESC, name COLLATE NOCASE, id
                        ) - 1 AS position
                        FROM bookmarks
                    )
                    UPDATE bookmarks
                    SET sort_order = (SELECT position FROM ordered WHERE ordered.id = bookmarks.id);",
                )?;
            }
            Ok(())
        })?;
        Ok(store)
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> Result<T, String> {
        let mut connection = Connection::open(&self.database_path).map_err(|error| error.to_string())?;
        connection.busy_timeout(DATABASE_BUSY_TIMEOUT).map_err(|error| error.to_string())?;
        operation(&mut connection).map_err(|error| error.to_string())
    }

    pub fn list(&self) -> Result<Vec<Bookmark>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, name, protocol, host, port, username, initial_path, key_path, key_passphrase_not_required, host_key, local_directory, tags,
                        s3_region, s3_endpoint, s3_force_path_style, s3_preserve_empty_directories,
                        smb_share, smb_domain, smb_guest, google_drive_location_kind, google_drive_location_id,
                        transfer_max_concurrent, transfer_bandwidth_limit_kbps, transfer_retry_count
                 FROM bookmarks ORDER BY sort_order ASC, name COLLATE NOCASE, id",
            )?;
            let bookmarks = statement
                .query_map([], |row| {
                    Ok(Bookmark {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        protocol: row.get(2)?,
                        host: row.get(3)?,
                        port: row.get(4)?,
                        username: row.get(5)?,
                        initial_path: row.get(6)?,
                        key_path: row.get(7)?,
                        key_passphrase_not_required: row.get(8)?,
                        host_key: row.get(9)?,
                        local_directory: row.get(10)?,
                        tags: row.get(11)?,
                        s3_region: row.get(12)?,
                        s3_endpoint: row.get(13)?,
                        s3_force_path_style: row.get(14)?,
                        s3_preserve_empty_directories: row.get(15)?,
                        smb_share: row.get(16)?,
                        smb_domain: row.get(17)?,
                        smb_guest: row.get(18)?,
                        google_drive_location_kind: row.get(19)?,
                        google_drive_location_id: row.get(20)?,
                        transfer_max_concurrent: row.get(21)?,
                        transfer_bandwidth_limit_kbps: row.get(22)?,
                        transfer_retry_count: row.get(23)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(bookmarks)
        })
    }

    pub fn save(&self, bookmark: &Bookmark) -> Result<(), String> {
        self.with_connection(|connection| {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let exists = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM bookmarks WHERE id = ?1)",
                params![bookmark.id],
                |row| row.get::<_, bool>(0),
            )?;
            if !exists {
                transaction.execute("UPDATE bookmarks SET sort_order = sort_order + 1", [])?;
            }
            transaction.execute(
                "INSERT INTO bookmarks (id, name, protocol, host, port, username, initial_path, key_path, key_passphrase_not_required, host_key, local_directory, tags, s3_region, s3_endpoint, s3_force_path_style, s3_preserve_empty_directories, smb_share, smb_domain, smb_guest, google_drive_location_kind, google_drive_location_id, transfer_max_concurrent, transfer_bandwidth_limit_kbps, transfer_retry_count, sort_order, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, 0, CURRENT_TIMESTAMP)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, protocol=excluded.protocol,
                 host=excluded.host, port=excluded.port, username=excluded.username,
                 initial_path=excluded.initial_path, key_path=excluded.key_path,
                 key_passphrase_not_required=excluded.key_passphrase_not_required, host_key=excluded.host_key,
                 local_directory=excluded.local_directory, tags=excluded.tags,
                 s3_region=excluded.s3_region, s3_endpoint=excluded.s3_endpoint,
                 s3_force_path_style=excluded.s3_force_path_style,
                 s3_preserve_empty_directories=excluded.s3_preserve_empty_directories,
                 smb_share=excluded.smb_share, smb_domain=excluded.smb_domain,
                 smb_guest=excluded.smb_guest,
                 google_drive_location_kind=excluded.google_drive_location_kind,
                 google_drive_location_id=excluded.google_drive_location_id,
                 transfer_max_concurrent=excluded.transfer_max_concurrent,
                 transfer_bandwidth_limit_kbps=excluded.transfer_bandwidth_limit_kbps,
                 transfer_retry_count=excluded.transfer_retry_count,
                 updated_at=CURRENT_TIMESTAMP",
                params![bookmark.id, bookmark.name, bookmark.protocol, bookmark.host, bookmark.port,
                    bookmark.username, bookmark.initial_path, bookmark.key_path,
                    bookmark.key_passphrase_not_required, bookmark.host_key, bookmark.local_directory,
                    bookmark.tags, bookmark.s3_region, bookmark.s3_endpoint,
                    bookmark.s3_force_path_style, bookmark.s3_preserve_empty_directories,
                    bookmark.smb_share, bookmark.smb_domain, bookmark.smb_guest,
                    bookmark.google_drive_location_kind, bookmark.google_drive_location_id,
                    bookmark.transfer_max_concurrent, bookmark.transfer_bandwidth_limit_kbps,
                    bookmark.transfer_retry_count],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn reorder(&self, bookmark_ids: &[String]) -> Result<(), String> {
        if bookmark_ids.len() > 10_000 {
            return Err("Too many bookmarks to reorder.".to_string());
        }
        self.with_connection(|connection| {
            let existing = connection
                .prepare("SELECT id FROM bookmarks")?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<HashSet<_>, _>>()?;
            let requested = bookmark_ids.iter().cloned().collect::<HashSet<_>>();
            if bookmark_ids.len() != existing.len() || requested != existing {
                return Err(rusqlite::Error::InvalidParameterName(
                    "Bookmark order must contain every bookmark exactly once.".to_string(),
                ));
            }
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            {
                let mut statement =
                    transaction.prepare("UPDATE bookmarks SET sort_order = ?1 WHERE id = ?2")?;
                for (position, id) in bookmark_ids.iter().enumerate() {
                    statement.execute(params![position as i64, id])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn record_history(&self, bookmark: &Bookmark) -> Result<(), String> {
        self.with_connection(|connection| {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO connection_history (bookmark_id, name, protocol, host, port, username)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    bookmark.id,
                    bookmark.name,
                    bookmark.protocol,
                    bookmark.host,
                    bookmark.port,
                    bookmark.username
                ],
            )?;
            transaction.execute(
                "DELETE FROM connection_history WHERE id NOT IN
                 (SELECT id FROM connection_history ORDER BY id DESC LIMIT 20)",
                [],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn history(&self) -> Result<Vec<ConnectionHistory>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT bookmark_id, name, protocol, host, port, username, connected_at
                 FROM connection_history ORDER BY id DESC LIMIT 20",
            )?;
            let history = statement
                .query_map([], |row| {
                    Ok(ConnectionHistory {
                        bookmark_id: row.get(0)?,
                        name: row.get(1)?,
                        protocol: row.get(2)?,
                        host: row.get(3)?,
                        port: row.get(4)?,
                        username: row.get(5)?,
                        connected_at: row.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(history)
        })
    }

    pub fn clear_history(&self) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM connection_history", [])?;
            Ok(())
        })
    }

    pub fn record_transfer(&self, transfer: &TransferHistory) -> Result<(), String> {
        self.with_connection(|connection| {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO transfer_history (id, name, direction, status, detail, bytes, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)
                 ON CONFLICT(id) DO UPDATE SET status=excluded.status, detail=excluded.detail,
                 bytes=excluded.bytes, completed_at=CURRENT_TIMESTAMP",
                params![
                    transfer.id,
                    transfer.name,
                    transfer.direction,
                    transfer.status,
                    transfer.detail,
                    transfer.bytes
                ],
            )?;
            transaction.execute(
                "DELETE FROM transfer_history WHERE id NOT IN
                 (SELECT id FROM transfer_history ORDER BY completed_at DESC LIMIT 100)",
                [],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn transfer_history(&self) -> Result<Vec<TransferHistory>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, name, direction, status, detail, bytes, completed_at
                 FROM transfer_history ORDER BY completed_at DESC LIMIT 100",
            )?;
            let history = statement
                .query_map([], |row| {
                    Ok(TransferHistory {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        direction: row.get(2)?,
                        status: row.get(3)?,
                        detail: row.get(4)?,
                        bytes: row.get(5)?,
                        completed_at: row.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(history)
        })
    }

    pub fn clear_transfer_history(&self) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM transfer_history", [])?;
            Ok(())
        })
    }

    pub fn save_transfer_job(&self, job: &TransferJob) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO transfer_jobs (id, connection_id, name, direction, local_path, remote_path, status, detail, transferred_bytes, total_bytes, retry_count, conflict_policy, is_directory, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, CURRENT_TIMESTAMP)
                 ON CONFLICT(id) DO UPDATE SET connection_id=excluded.connection_id,
                 name=excluded.name, direction=excluded.direction, local_path=excluded.local_path,
                 remote_path=excluded.remote_path, status=excluded.status, detail=excluded.detail,
                 transferred_bytes=excluded.transferred_bytes, total_bytes=excluded.total_bytes,
                 retry_count=transfer_jobs.retry_count + 1, conflict_policy=excluded.conflict_policy,
                 is_directory=excluded.is_directory, updated_at=CURRENT_TIMESTAMP",
                params![job.id, job.connection_id, job.name, job.direction, job.local_path,
                    job.remote_path, job.status, job.detail, job.transferred_bytes, job.total_bytes,
                    job.retry_count, job.conflict_policy, job.is_directory],
            )?;
            Ok(())
        })
    }

    pub fn update_transfer_job_progress(&self, id: &str, transferred: u64, total: u64) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
                "UPDATE transfer_jobs SET transferred_bytes=?2, total_bytes=?3, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                params![id, transferred, total],
            )?;
            Ok(())
        })
    }

    pub fn transfer_job_checkpoint(&self, id: &str) -> Result<Option<(u64, u64, u32)>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT transferred_bytes, total_bytes, retry_count FROM transfer_jobs WHERE id=?1",
            )?;
            let mut rows = statement.query(params![id])?;
            rows.next()?.map(|row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).transpose()
        })
    }

    pub fn set_transfer_job_status(&self, id: &str, status: &str, detail: &str) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
                "UPDATE transfer_jobs SET status=?2, detail=?3,
                 updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                params![id, status, detail],
            )?;
            Ok(())
        })
    }

    pub fn set_transfer_job_retry(&self, id: &str, retry_count: u32, detail: &str) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
                "UPDATE transfer_jobs SET status='Running', retry_count=?2, detail=?3, updated_at=CURRENT_TIMESTAMP WHERE id=?1",
                params![id, retry_count, detail],
            )?;
            Ok(())
        })
    }

    pub fn delete_transfer_job(&self, id: &str) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM s3_multipart_uploads WHERE transfer_id=?1", params![id])?;
            connection.execute("DELETE FROM transfer_jobs WHERE id=?1", params![id])?;
            Ok(())
        })
    }

    pub fn save_s3_multipart_state(&self, transfer_id: &str, state_json: &str) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO s3_multipart_uploads (transfer_id, state_json, updated_at)
                 VALUES (?1, ?2, CURRENT_TIMESTAMP)
                 ON CONFLICT(transfer_id) DO UPDATE SET state_json=excluded.state_json,
                 updated_at=CURRENT_TIMESTAMP",
                params![transfer_id, state_json],
            )?;
            Ok(())
        })
    }

    pub fn s3_multipart_state(&self, transfer_id: &str) -> Result<Option<String>, String> {
        self.with_connection(|connection| {
            let mut statement =
                connection.prepare("SELECT state_json FROM s3_multipart_uploads WHERE transfer_id=?1")?;
            let mut rows = statement.query(params![transfer_id])?;
            rows.next()?.map(|row| row.get(0)).transpose()
        })
    }

    pub fn delete_s3_multipart_state(&self, transfer_id: &str) -> Result<(), String> {
        self.with_connection(|connection| {
            connection
                .execute("DELETE FROM s3_multipart_uploads WHERE transfer_id=?1", params![transfer_id])?;
            Ok(())
        })
    }

    pub fn transfer_jobs(&self) -> Result<Vec<TransferJob>, String> {
        self.with_connection(|connection| {
            connection.execute(
                "UPDATE transfer_jobs SET status='Failed',
                 detail='Transfer was interrupted when Harbor Transfer closed.',
                 updated_at=CURRENT_TIMESTAMP WHERE status IN ('Running', 'Queued', 'Paused')",
                [],
            )?;
            let mut statement = connection.prepare(
                "SELECT id, connection_id, name, direction, local_path, remote_path, status,
                        detail, transferred_bytes, total_bytes, retry_count, conflict_policy,
                        is_directory, updated_at
                 FROM transfer_jobs ORDER BY updated_at DESC LIMIT 1000",
            )?;
            let jobs = statement
                .query_map([], |row| {
                    Ok(TransferJob {
                        id: row.get(0)?,
                        connection_id: row.get(1)?,
                        name: row.get(2)?,
                        direction: row.get(3)?,
                        local_path: row.get(4)?,
                        remote_path: row.get(5)?,
                        status: row.get(6)?,
                        detail: row.get(7)?,
                        transferred_bytes: row.get(8)?,
                        total_bytes: row.get(9)?,
                        retry_count: row.get(10)?,
                        conflict_policy: row.get(11)?,
                        is_directory: row.get(12)?,
                        updated_at: row.get(13)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(jobs)
        })
    }

    pub fn clear_transfer_jobs(&self) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
                "DELETE FROM s3_multipart_uploads WHERE transfer_id IN
                 (SELECT id FROM transfer_jobs WHERE status NOT IN ('Running', 'Paused', 'Queued'))",
                [],
            )?;
            connection.execute(
                "DELETE FROM transfer_jobs WHERE status NOT IN ('Running', 'Paused', 'Queued')",
                [],
            )?;
            Ok(())
        })
    }

    pub fn record_sync_history(&self, sync: &SyncHistory) -> Result<(), String> {
        self.with_connection(|connection| {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO sync_history (id, direction, local_directory, remote_directory, status, completed_items, total_items, bytes, detail, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP)
                 ON CONFLICT(id) DO UPDATE SET status=excluded.status,
                 completed_items=excluded.completed_items, total_items=excluded.total_items,
                 bytes=excluded.bytes, detail=excluded.detail, completed_at=CURRENT_TIMESTAMP",
                params![
                    sync.id,
                    sync.direction,
                    sync.local_directory,
                    sync.remote_directory,
                    sync.status,
                    sync.completed_items,
                    sync.total_items,
                    sync.bytes,
                    sync.detail
                ],
            )?;
            transaction.execute(
                "DELETE FROM sync_history WHERE id NOT IN
                 (SELECT id FROM sync_history ORDER BY completed_at DESC LIMIT 50)",
                [],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn sync_history(&self) -> Result<Vec<SyncHistory>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, direction, local_directory, remote_directory, status,
                        completed_items, total_items, bytes, detail, completed_at
                 FROM sync_history ORDER BY completed_at DESC LIMIT 50",
            )?;
            let history = statement
                .query_map([], |row| {
                    Ok(SyncHistory {
                        id: row.get(0)?,
                        direction: row.get(1)?,
                        local_directory: row.get(2)?,
                        remote_directory: row.get(3)?,
                        status: row.get(4)?,
                        completed_items: row.get(5)?,
                        total_items: row.get(6)?,
                        bytes: row.get(7)?,
                        detail: row.get(8)?,
                        completed_at: row.get(9)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(history)
        })
    }

    pub fn clear_sync_history(&self) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM sync_history", [])?;
            Ok(())
        })
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Bookmark, BookmarkStore, SyncHistory, TransferHistory, TransferJob};

    fn sample_bookmark() -> Bookmark {
        Bookmark {
            id: "bookmark-1".to_string(),
            name: "Example SFTP".to_string(),
            protocol: "sftp".to_string(),
            host: "example.com".to_string(),
            port: 22,
            username: "alice".to_string(),
            initial_path: "/deploy".to_string(),
            key_path: Some("/Users/alice/.ssh/id_ed25519".to_string()),
            key_passphrase_not_required: false,
            host_key: Some("SHA256:example".to_string()),
            local_directory: Some("/Users/alice/Sites/example".to_string()),
            tags: "production,web".to_string(),
            s3_region: None,
            s3_endpoint: None,
            s3_force_path_style: false,
            s3_preserve_empty_directories: false,
            smb_share: None,
            smb_domain: None,
            smb_guest: false,
            google_drive_location_kind: None,
            google_drive_location_id: None,
            transfer_max_concurrent: None,
            transfer_bandwidth_limit_kbps: None,
            transfer_retry_count: None,
        }
    }

    #[test]
    fn saves_and_updates_a_bookmark_without_secret_fields() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let mut bookmark = sample_bookmark();
        store.save(&bookmark).expect("save bookmark");

        bookmark.name = "Production".to_string();
        bookmark.key_passphrase_not_required = true;
        bookmark.s3_preserve_empty_directories = true;
        bookmark.protocol = "smb".to_string();
        bookmark.smb_share = Some("Documents".to_string());
        bookmark.smb_domain = Some("WORKGROUP".to_string());
        bookmark.smb_guest = true;
        bookmark.google_drive_location_kind = Some("sharedDrive".to_string());
        bookmark.google_drive_location_id = Some("drive-123".to_string());
        bookmark.transfer_max_concurrent = Some(2);
        bookmark.transfer_bandwidth_limit_kbps = Some(2048);
        bookmark.transfer_retry_count = Some(5);
        store.save(&bookmark).expect("update bookmark");
        let bookmarks = store.list().expect("list bookmarks");

        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].name, "Production");
        assert_eq!(bookmarks[0].host_key.as_deref(), Some("SHA256:example"));
        assert_eq!(bookmarks[0].local_directory.as_deref(), Some("/Users/alice/Sites/example"));
        assert!(bookmarks[0].key_passphrase_not_required);
        assert!(bookmarks[0].s3_preserve_empty_directories);
        assert_eq!(bookmarks[0].smb_share.as_deref(), Some("Documents"));
        assert_eq!(bookmarks[0].smb_domain.as_deref(), Some("WORKGROUP"));
        assert!(bookmarks[0].smb_guest);
        assert_eq!(bookmarks[0].google_drive_location_kind.as_deref(), Some("sharedDrive"));
        assert_eq!(bookmarks[0].google_drive_location_id.as_deref(), Some("drive-123"));
        assert_eq!(bookmarks[0].transfer_max_concurrent, Some(2));
        assert_eq!(bookmarks[0].transfer_bandwidth_limit_kbps, Some(2048));
        assert_eq!(bookmarks[0].transfer_retry_count, Some(5));
    }

    #[test]
    fn reorders_bookmarks_and_keeps_the_order_when_editing() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let mut first = sample_bookmark();
        first.id = "bookmark-first".to_string();
        first.name = "First".to_string();
        store.save(&first).expect("save first bookmark");
        let mut second = sample_bookmark();
        second.id = "bookmark-second".to_string();
        second.name = "Second".to_string();
        store.save(&second).expect("save second bookmark");

        assert_eq!(
            store.list().expect("list newest first").iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec!["bookmark-second", "bookmark-first"]
        );
        store.reorder(&[first.id.clone(), second.id.clone()]).expect("reorder bookmarks");
        second.name = "Second edited".to_string();
        store.save(&second).expect("edit reordered bookmark");

        let ordered = store.list().expect("list reordered bookmarks");
        assert_eq!(
            ordered.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec!["bookmark-first", "bookmark-second"]
        );
        assert_eq!(ordered[1].name, "Second edited");
        assert!(store.reorder(&[first.id]).is_err());
    }

    #[test]
    fn migrates_existing_bookmarks_in_their_previous_display_order() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let database_path = directory.path().join("harbor-transfer.sqlite3");
        {
            let connection = rusqlite::Connection::open(&database_path).expect("legacy database");
            connection
                .execute_batch(
                    "CREATE TABLE bookmarks (
                        id TEXT PRIMARY KEY NOT NULL,
                        name TEXT NOT NULL,
                        protocol TEXT NOT NULL,
                        host TEXT NOT NULL,
                        port INTEGER NOT NULL,
                        username TEXT NOT NULL,
                        initial_path TEXT NOT NULL DEFAULT '/',
                        key_path TEXT,
                        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                    );
                    INSERT INTO bookmarks (id, name, protocol, host, port, username, updated_at)
                    VALUES
                        ('older', 'Older', 'sftp', 'older.example.com', 22, 'alice', '2026-01-01 00:00:00'),
                        ('newer', 'Newer', 'sftp', 'newer.example.com', 22, 'alice', '2026-02-01 00:00:00');",
                )
                .expect("create legacy bookmarks");
        }

        let store = BookmarkStore::new(directory.path()).expect("migrate bookmark store");
        assert_eq!(
            store
                .list()
                .expect("list migrated bookmarks")
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["newer", "older"]
        );
    }

    #[test]
    fn deletes_a_bookmark() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let bookmark = sample_bookmark();
        store.save(&bookmark).expect("save bookmark");
        store.delete(&bookmark.id).expect("delete bookmark");
        assert!(store.list().expect("list bookmarks").is_empty());
    }

    #[test]
    fn records_connection_history_in_newest_first_order() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let bookmark = sample_bookmark();
        store.record_history(&bookmark).expect("record history");
        let history = store.history().expect("list history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].bookmark_id, bookmark.id);
        assert_eq!(history[0].host, "example.com");
        store.clear_history().expect("clear history");
        assert!(store.history().expect("list cleared history").is_empty());
    }

    #[test]
    fn waits_for_a_transient_database_writer_instead_of_returning_locked() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let database_path = store.database_path.clone();
        let lock_connection = rusqlite::Connection::open(&database_path).expect("lock connection");
        lock_connection.execute_batch("BEGIN IMMEDIATE").expect("hold write lock");

        let bookmark = sample_bookmark();
        let writer = std::thread::spawn(move || {
            let concurrent_store = BookmarkStore { database_path };
            concurrent_store.save(&bookmark)
        });
        std::thread::sleep(std::time::Duration::from_millis(120));
        lock_connection.execute_batch("COMMIT").expect("release write lock");

        writer.join().expect("writer thread").expect("wait for write lock");
        assert_eq!(store.list().expect("list bookmarks").len(), 1);
    }

    #[test]
    fn records_and_updates_transfer_history() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let mut transfer = TransferHistory {
            id: "transfer-1".to_string(),
            name: "release.zip".to_string(),
            direction: "Upload".to_string(),
            status: "Completed".to_string(),
            detail: "/release.zip".to_string(),
            bytes: 42,
            completed_at: String::new(),
        };
        store.record_transfer(&transfer).expect("record transfer");
        transfer.status = "Failed".to_string();
        transfer.detail = "network error".to_string();
        store.record_transfer(&transfer).expect("update transfer");

        let history = store.transfer_history().expect("list transfer history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "Failed");
        assert_eq!(history[0].bytes, 42);
        store.clear_transfer_history().expect("clear transfer history");
        assert!(store.transfer_history().expect("list cleared transfer history").is_empty());
    }

    #[test]
    fn restores_interrupted_transfer_jobs_with_retry_context() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let job = TransferJob {
            id: "transfer-1".to_string(),
            connection_id: "bookmark-1".to_string(),
            name: "archive.zip".to_string(),
            direction: "Upload".to_string(),
            local_path: "/tmp/archive.zip".to_string(),
            remote_path: "/uploads/archive.zip".to_string(),
            status: "Running".to_string(),
            detail: String::new(),
            transferred_bytes: 4_194_304,
            total_bytes: 16_777_216,
            retry_count: 0,
            conflict_policy: "overwrite".to_string(),
            is_directory: false,
            updated_at: String::new(),
        };
        store.save_transfer_job(&job).expect("save running job");
        let multipart = r#"{"uploadId":"upload-1","parts":[1]}"#;
        store.save_s3_multipart_state(&job.id, multipart).expect("save multipart state");
        store.update_transfer_job_progress(&job.id, 8_388_608, job.total_bytes).expect("persist progress");
        store
            .set_transfer_job_retry(&job.id, 2, "Temporary connection failure; reconnecting")
            .expect("persist automatic retry context");
        assert_eq!(
            store.transfer_job_checkpoint(&job.id).expect("read checkpoint"),
            Some((8_388_608, job.total_bytes, 2))
        );

        let restored = store.transfer_jobs().expect("restore transfer jobs");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].status, "Failed");
        assert!(restored[0].detail.contains("interrupted"));
        assert_eq!(restored[0].transferred_bytes, 8_388_608);
        assert_eq!(restored[0].retry_count, 2);
        assert_eq!(restored[0].connection_id, "bookmark-1");
        assert_eq!(restored[0].conflict_policy, "overwrite");
        assert_eq!(store.s3_multipart_state(&job.id).unwrap().as_deref(), Some(multipart));

        store.clear_transfer_jobs().expect("clear restored jobs");
        assert!(store.transfer_jobs().expect("list cleared jobs").is_empty());
        assert!(store.s3_multipart_state(&job.id).unwrap().is_none());
    }

    #[test]
    fn records_sync_execution_history() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let sync = SyncHistory {
            id: "sync-1".to_string(),
            direction: "localToRemote".to_string(),
            local_directory: "/tmp/local".to_string(),
            remote_directory: "/remote".to_string(),
            status: "Completed".to_string(),
            completed_items: 2,
            total_items: 2,
            bytes: 42,
            detail: "[]".to_string(),
            completed_at: String::new(),
        };
        store.record_sync_history(&sync).expect("record sync");
        let history = store.sync_history().expect("sync history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].completed_items, 2);
        store.clear_sync_history().expect("clear sync history");
        assert!(store.sync_history().expect("cleared sync history").is_empty());
    }
}

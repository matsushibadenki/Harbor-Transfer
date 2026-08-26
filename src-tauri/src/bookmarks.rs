use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

pub struct BookmarkStore {
    database_path: PathBuf,
}

impl BookmarkStore {
    pub fn new(data_directory: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(data_directory)
            .map_err(|error| format!("Failed to create app data directory: {error}"))?;
        let store = Self { database_path: data_directory.join("harbor-transfer.sqlite3") };
        store.with_connection(|connection| {
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
            Ok(())
        })?;
        Ok(store)
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, String> {
        let connection = Connection::open(&self.database_path).map_err(|error| error.to_string())?;
        operation(&connection).map_err(|error| error.to_string())
    }

    pub fn list(&self) -> Result<Vec<Bookmark>, String> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, name, protocol, host, port, username, initial_path, key_path, key_passphrase_not_required, host_key, local_directory, tags,
                        s3_region, s3_endpoint, s3_force_path_style, s3_preserve_empty_directories
                 FROM bookmarks ORDER BY updated_at DESC, name COLLATE NOCASE",
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
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(bookmarks)
        })
    }

    pub fn save(&self, bookmark: &Bookmark) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO bookmarks (id, name, protocol, host, port, username, initial_path, key_path, key_passphrase_not_required, host_key, local_directory, tags, s3_region, s3_endpoint, s3_force_path_style, s3_preserve_empty_directories, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, CURRENT_TIMESTAMP)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, protocol=excluded.protocol,
                 host=excluded.host, port=excluded.port, username=excluded.username,
                 initial_path=excluded.initial_path, key_path=excluded.key_path,
                 key_passphrase_not_required=excluded.key_passphrase_not_required, host_key=excluded.host_key,
                 local_directory=excluded.local_directory, tags=excluded.tags,
                 s3_region=excluded.s3_region, s3_endpoint=excluded.s3_endpoint,
                 s3_force_path_style=excluded.s3_force_path_style,
                 s3_preserve_empty_directories=excluded.s3_preserve_empty_directories,
                 updated_at=CURRENT_TIMESTAMP",
                params![bookmark.id, bookmark.name, bookmark.protocol, bookmark.host, bookmark.port,
                    bookmark.username, bookmark.initial_path, bookmark.key_path,
                    bookmark.key_passphrase_not_required, bookmark.host_key, bookmark.local_directory,
                    bookmark.tags, bookmark.s3_region, bookmark.s3_endpoint,
                    bookmark.s3_force_path_style, bookmark.s3_preserve_empty_directories],
            )?;
            Ok(())
        })
    }

    pub fn record_history(&self, bookmark: &Bookmark) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
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
            connection.execute(
                "DELETE FROM connection_history WHERE id NOT IN
                 (SELECT id FROM connection_history ORDER BY id DESC LIMIT 20)",
                [],
            )?;
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
            connection.execute(
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
            connection.execute(
                "DELETE FROM transfer_history WHERE id NOT IN
                 (SELECT id FROM transfer_history ORDER BY completed_at DESC LIMIT 100)",
                [],
            )?;
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

    pub fn record_sync_history(&self, sync: &SyncHistory) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
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
            connection.execute(
                "DELETE FROM sync_history WHERE id NOT IN
                 (SELECT id FROM sync_history ORDER BY completed_at DESC LIMIT 50)",
                [],
            )?;
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
    use super::{Bookmark, BookmarkStore, SyncHistory, TransferHistory};

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
        store.save(&bookmark).expect("update bookmark");
        let bookmarks = store.list().expect("list bookmarks");

        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].name, "Production");
        assert_eq!(bookmarks[0].host_key.as_deref(), Some("SHA256:example"));
        assert_eq!(bookmarks[0].local_directory.as_deref(), Some("/Users/alice/Sites/example"));
        assert!(bookmarks[0].key_passphrase_not_required);
        assert!(bookmarks[0].s3_preserve_empty_directories);
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

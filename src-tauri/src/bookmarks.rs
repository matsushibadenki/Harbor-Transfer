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
    pub host_key: Option<String>,
    #[serde(default)]
    pub local_directory: Option<String>,
    #[serde(default)]
    pub tags: String,
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
                    host_key TEXT,
                    local_directory TEXT,
                    tags TEXT NOT NULL DEFAULT '',
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
                );",
            )?;
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN host_key TEXT", []);
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN local_directory TEXT", []);
            let _ = connection.execute("ALTER TABLE bookmarks ADD COLUMN tags TEXT NOT NULL DEFAULT ''", []);
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
                "SELECT id, name, protocol, host, port, username, initial_path, key_path, host_key, local_directory, tags
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
                        host_key: row.get(8)?,
                        local_directory: row.get(9)?,
                        tags: row.get(10)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(bookmarks)
        })
    }

    pub fn save(&self, bookmark: &Bookmark) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO bookmarks (id, name, protocol, host, port, username, initial_path, key_path, host_key, local_directory, tags, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, CURRENT_TIMESTAMP)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, protocol=excluded.protocol,
                 host=excluded.host, port=excluded.port, username=excluded.username,
                 initial_path=excluded.initial_path, key_path=excluded.key_path, host_key=excluded.host_key,
                 local_directory=excluded.local_directory, tags=excluded.tags, updated_at=CURRENT_TIMESTAMP",
                params![bookmark.id, bookmark.name, bookmark.protocol, bookmark.host, bookmark.port,
                    bookmark.username, bookmark.initial_path, bookmark.key_path, bookmark.host_key,
                    bookmark.local_directory, bookmark.tags],
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

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Bookmark, BookmarkStore, TransferHistory};

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
            host_key: Some("SHA256:example".to_string()),
            local_directory: Some("/Users/alice/Sites/example".to_string()),
            tags: "production,web".to_string(),
        }
    }

    #[test]
    fn saves_and_updates_a_bookmark_without_secret_fields() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = BookmarkStore::new(directory.path()).expect("bookmark store");
        let mut bookmark = sample_bookmark();
        store.save(&bookmark).expect("save bookmark");

        bookmark.name = "Production".to_string();
        store.save(&bookmark).expect("update bookmark");
        let bookmarks = store.list().expect("list bookmarks");

        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].name, "Production");
        assert_eq!(bookmarks[0].host_key.as_deref(), Some("SHA256:example"));
        assert_eq!(bookmarks[0].local_directory.as_deref(), Some("/Users/alice/Sites/example"));
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
}

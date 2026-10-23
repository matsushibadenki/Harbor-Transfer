use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static PLACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const MAX_PLACES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlace {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub bookmark_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePlaceRequest {
    pub id: Option<String>,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub bookmark_id: String,
    pub path: String,
}

pub struct PlaceStore {
    path: PathBuf,
}

impl PlaceStore {
    pub fn load_workspace(
        &self,
        window: &str,
    ) -> Result<Option<crate::workspace_tabs::WorkspaceDocument>, String> {
        let connection = self.connect()?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS workspaces (window TEXT PRIMARY KEY, document TEXT NOT NULL)",
            )
            .map_err(|e| e.to_string())?;
        let json: Option<String> = connection
            .query_row("SELECT document FROM workspaces WHERE window=?1", [window], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        json.map(|json| {
            let document: crate::workspace_tabs::WorkspaceDocument =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            document.validate()?;
            Ok(document)
        })
        .transpose()
    }

    pub fn save_workspace(
        &self,
        window: &str,
        document: crate::workspace_tabs::WorkspaceDocument,
    ) -> Result<(), String> {
        document.validate()?;
        let json = serde_json::to_string(&document).map_err(|e| e.to_string())?;
        let connection = self.connect()?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS workspaces (window TEXT PRIMARY KEY, document TEXT NOT NULL)",
            )
            .map_err(|e| e.to_string())?;
        connection.execute("INSERT INTO workspaces(window,document) VALUES(?1,?2) ON CONFLICT(window) DO UPDATE SET document=excluded.document", params![window,json]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn new(directory: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let store = Self { path: directory.join("workspace.sqlite3") };
        store
            .connect()?
            .execute_batch(
                "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS places (
               id TEXT PRIMARY KEY NOT NULL,
               label TEXT NOT NULL,
               kind TEXT NOT NULL CHECK(kind IN ('local','remote')),
               bookmark_id TEXT NOT NULL,
               path TEXT NOT NULL,
               UNIQUE(kind, bookmark_id, path)
             );",
            )
            .map_err(|error| error.to_string())?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, String> {
        let connection = Connection::open(&self.path).map_err(|error| error.to_string())?;
        connection.busy_timeout(Duration::from_secs(8)).map_err(|error| error.to_string())?;
        Ok(connection)
    }

    pub fn list(&self) -> Result<Vec<SavedPlace>, String> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT id, label, kind, bookmark_id, path FROM places ORDER BY rowid")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(SavedPlace {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    kind: row.get(2)?,
                    bookmark_id: row.get(3)?,
                    path: row.get(4)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())
    }

    pub fn save(&self, request: SavePlaceRequest) -> Result<(), String> {
        let label = request.label.trim();
        if label.is_empty() || label.chars().count() > 128 || label.chars().any(char::is_control) {
            return Err("Place labels must contain 1–128 printable characters.".into());
        }
        if !matches!(request.kind.as_str(), "local" | "remote")
            || (request.kind == "local" && !request.bookmark_id.is_empty())
            || (request.kind == "remote" && request.bookmark_id.is_empty())
            || request.bookmark_id.len() > 256
            || !request.path.starts_with('/')
            || request.path.len() > 16_384
            || request.path.contains('\0')
        {
            return Err("Invalid place location.".into());
        }
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        if let Some(id) = request.id {
            let changed = transaction
                .execute(
                    "UPDATE places SET label=?1 WHERE id=?2 AND kind=?3 AND bookmark_id=?4 AND path=?5",
                    params![label, id, request.kind, request.bookmark_id, request.path],
                )
                .map_err(|error| error.to_string())?;
            if changed != 1 {
                return Err("Saved place no longer exists.".into());
            }
        } else {
            let existing: Option<String> = transaction
                .query_row(
                    "SELECT id FROM places WHERE kind=?1 AND bookmark_id=?2 AND path=?3",
                    params![request.kind, request.bookmark_id, request.path],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(id) = existing {
                transaction
                    .execute("UPDATE places SET label=?1 WHERE id=?2", params![label, id])
                    .map_err(|error| error.to_string())?;
            } else {
                let count: usize = transaction
                    .query_row("SELECT COUNT(*) FROM places", [], |row| row.get(0))
                    .map_err(|error| error.to_string())?;
                if count >= MAX_PLACES {
                    return Err("You can save up to 200 places.".into());
                }
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| error.to_string())?
                    .as_millis();
                let id = format!("place-{now}-{}", PLACE_SEQUENCE.fetch_add(1, Ordering::Relaxed));
                transaction
                    .execute(
                        "INSERT INTO places(id,label,kind,bookmark_id,path) VALUES(?1,?2,?3,?4,?5)",
                        params![id, label, request.kind, request.bookmark_id, request.path],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        transaction.commit().map_err(|error| error.to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.connect()?.execute("DELETE FROM places WHERE id=?1", [id]).map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkFailure {
    OutsideRoot,
    Missing,
    Ambiguous,
    NotDirectory,
    InvalidPath,
    Failed,
}

pub fn relative_names(root: &str, path: &str) -> Result<Vec<String>, LinkFailure> {
    fn components(path: &str) -> Result<Vec<&str>, LinkFailure> {
        if !path.starts_with('/') || path.len() > 16_384 || path.contains('\0') {
            return Err(LinkFailure::InvalidPath);
        }
        let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
        if parts.len() > 128 || parts.iter().any(|part| matches!(*part, "." | "..")) {
            return Err(LinkFailure::InvalidPath);
        }
        Ok(parts)
    }
    let root = components(root)?;
    let path = components(path)?;
    if !path.starts_with(&root) {
        return Err(LinkFailure::OutsideRoot);
    }
    path[root.len()..]
        .iter()
        .map(|component| {
            let name = if let Some(encoded) = component.strip_prefix("~gdrive~") {
                let (name, _) = encoded.rsplit_once('~').ok_or(LinkFailure::InvalidPath)?;
                let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(name)
                    .map_err(|_| LinkFailure::InvalidPath)?;
                String::from_utf8(bytes).map_err(|_| LinkFailure::InvalidPath)?
            } else if let Some((name, _)) = component.rsplit_once('\u{1f}') {
                name.to_string()
            } else {
                component.to_string()
            };
            if name.is_empty() || matches!(name.as_str(), "." | "..") || name.contains('\0') {
                Err(LinkFailure::InvalidPath)
            } else {
                Ok(name)
            }
        })
        .collect()
}

pub fn resolve_local(root: &str, names: &[String]) -> Result<String, LinkFailure> {
    fn io_failure(error: std::io::Error) -> LinkFailure {
        if error.kind() == std::io::ErrorKind::NotFound {
            LinkFailure::Missing
        } else {
            LinkFailure::Failed
        }
    }
    let root = std::fs::canonicalize(root).map_err(io_failure)?;
    let mut path = root.clone();
    if !path.is_dir() {
        return Err(LinkFailure::NotDirectory);
    }
    for name in names {
        if name.contains('/') || name.contains('\\') || matches!(name.as_str(), "." | "..") {
            return Err(LinkFailure::InvalidPath);
        }
        path.push(name);
        let metadata = std::fs::symlink_metadata(&path).map_err(io_failure)?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(LinkFailure::NotDirectory);
        }
        path = std::fs::canonicalize(&path).map_err(io_failure)?;
        if !path.starts_with(&root) {
            return Err(LinkFailure::OutsideRoot);
        }
    }
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(label: &str, kind: &str, bookmark: &str) -> SavePlaceRequest {
        SavePlaceRequest {
            id: None,
            label: label.into(),
            kind: kind.into(),
            bookmark_id: bookmark.into(),
            path: "/projects".into(),
        }
    }

    #[test]
    fn places_persist_and_deduplicate_by_location() {
        let directory = tempfile::tempdir().unwrap();
        let store = PlaceStore::new(directory.path()).unwrap();
        store.save(request("Work", "local", "")).unwrap();
        store.save(request("仕事", "local", "")).unwrap();
        store.save(request("Remote", "remote", "server-a")).unwrap();
        store.save(request("Remote", "remote", "server-b")).unwrap();
        drop(store);
        let store = PlaceStore::new(directory.path()).unwrap();
        let places = store.list().unwrap();
        assert_eq!(places.len(), 3);
        assert_eq!(places[0].label, "仕事");
        store.delete(&places[0].id).unwrap();
        assert_eq!(store.list().unwrap().len(), 2);
    }

    #[test]
    fn places_validate_and_rename_without_changing_location() {
        let directory = tempfile::tempdir().unwrap();
        let store = PlaceStore::new(directory.path()).unwrap();
        assert!(store.save(request("", "local", "")).is_err());
        assert!(store.save(request("X", "remote", "")).is_err());
        assert!(store.save(request("X", "local", "server")).is_err());
        store.save(request("Work", "local", "")).unwrap();
        let place = store.list().unwrap().remove(0);
        let mut renamed = request("'中文'; DROP TABLE places; --", "local", "");
        renamed.id = Some(place.id.clone());
        store.save(renamed).unwrap();
        assert_eq!(store.list().unwrap()[0].id, place.id);
        let mut stale = request("Other", "local", "");
        stale.id = Some("missing".into());
        assert!(store.save(stale).is_err());
    }

    #[test]
    fn linked_paths_respect_root_boundaries_and_drive_ids() {
        assert_eq!(relative_names("/home/site", "/home/site/assets/icons").unwrap(), ["assets", "icons"]);
        assert_eq!(relative_names("/home/site", "/home/site-other").unwrap_err(), LinkFailure::OutsideRoot);
        assert!(relative_names("/home", "/home/../outside").is_err());
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("資料");
        assert_eq!(relative_names("/", &format!("/~gdrive~{encoded}~drive-id")).unwrap(), ["資料"]);
    }

    #[test]
    fn place_limit_does_not_prevent_renaming_or_updating_existing_places() {
        let directory = tempfile::tempdir().unwrap();
        let store = PlaceStore::new(directory.path()).unwrap();
        for index in 0..MAX_PLACES {
            let mut place = request("Folder", "local", "");
            place.path = format!("/folder-{index}");
            store.save(place).unwrap();
        }
        assert!(store.save(request("Overflow", "local", "")).is_err());
        let mut existing = request("Updated", "local", "");
        existing.path = "/folder-0".into();
        store.save(existing).unwrap();
        assert_eq!(store.list().unwrap().len(), MAX_PLACES);
        assert_eq!(store.list().unwrap()[0].label, "Updated");
    }

    #[test]
    fn linked_local_navigation_never_creates_directories_or_follows_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("assets")).unwrap();
        let root = directory.path().to_str().unwrap();
        assert!(resolve_local(root, &["assets".into()]).is_ok());
        assert_eq!(resolve_local(root, &["missing".into()]).unwrap_err(), LinkFailure::Missing);
        assert!(!directory.path().join("missing").exists());
        assert!(resolve_local(root, &["..".into()]).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(directory.path().join("assets"), directory.path().join("link"))
                .unwrap();
            assert_eq!(resolve_local(root, &["link".into()]).unwrap_err(), LinkFailure::NotDirectory);
        }
    }
}

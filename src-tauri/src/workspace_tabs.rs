//! Credential-free, versioned workspace documents. Window labels isolate live windows.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabPane {
    pub kind: String,
    pub bookmark_id: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceTab {
    pub id: String,
    pub name: String,
    pub primary: TabPane,
    pub auxiliary: TabPane,
    pub dual_pane: bool,
    pub left_pane_kind: String,
    pub ratio: f64,
    pub view_mode: String,
    pub widths: BTreeMap<String, f64>,
    pub visibility: BTreeMap<String, bool>,
    pub sort_column: String,
    pub sort_direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceDocument {
    pub version: u32,
    pub active_id: String,
    pub tabs: Vec<WorkspaceTab>,
}

impl WorkspaceDocument {
    pub fn validate(&self) -> Result<(), String> {
        let columns = ["name", "size", "modified", "permissions", "owner", "group", "type"];
        if self.version != 1
            || self.tabs.is_empty()
            || self.tabs.len() > 32
            || !self.tabs.iter().any(|tab| tab.id == self.active_id)
        {
            return Err("Invalid workspace document.".into());
        }
        let mut ids = std::collections::HashSet::new();
        for tab in &self.tabs {
            if tab.id.is_empty()
                || tab.id.len() > 128
                || !ids.insert(&tab.id)
                || tab.name.chars().count() > 80
                || tab.name.chars().any(char::is_control)
                || !matches!(tab.view_mode.as_str(), "list" | "icons" | "columns")
                || !matches!(tab.left_pane_kind.as_str(), "local" | "remote")
                || !tab.ratio.is_finite()
                || !(0.15..=0.85).contains(&tab.ratio)
                || !columns.contains(&tab.sort_column.as_str())
                || !matches!(tab.sort_direction.as_str(), "ascending" | "descending")
                || tab.widths.len() != 7
                || tab.widths.iter().any(|(key, width)| {
                    !columns.contains(&key.as_str())
                        || !width.is_finite()
                        || !(24.0..=10000.0).contains(width)
                })
                || tab.visibility.len() != 6
                || tab.visibility.keys().any(|key| key == "name" || !columns.contains(&key.as_str()))
            {
                return Err("Invalid workspace tab.".into());
            }
            for pane in [&tab.primary, &tab.auxiliary] {
                if !matches!(pane.kind.as_str(), "local" | "remote")
                    || !pane.path.starts_with('/')
                    || pane.path.len() > 16384
                    || pane.path.contains('\0')
                    || pane.bookmark_id.as_ref().is_some_and(|id| id.is_empty() || id.len() > 256)
                    || (pane.kind == "local" && pane.bookmark_id.is_some())
                {
                    return Err("Invalid workspace pane.".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> WorkspaceDocument {
        let columns = ["name", "size", "modified", "permissions", "owner", "group", "type"];
        let pane = TabPane { kind: "local".into(), bookmark_id: None, path: "/Users/example".into() };
        WorkspaceDocument {
            version: 1,
            active_id: "a".into(),
            tabs: vec![WorkspaceTab {
                id: "a".into(),
                name: "制作・Development".into(),
                primary: pane.clone(),
                auxiliary: pane,
                dual_pane: true,
                left_pane_kind: "local".into(),
                ratio: 0.5,
                view_mode: "icons".into(),
                widths: columns.iter().map(|key| (key.to_string(), 120.0)).collect(),
                visibility: columns[1..].iter().map(|key| (key.to_string(), true)).collect(),
                sort_column: "name".into(),
                sort_direction: "ascending".into(),
            }],
        }
    }
    #[test]
    fn workspace_round_trip_and_window_isolation() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::workspace::PlaceStore::new(directory.path()).unwrap();
        let doc = document();
        store.save_workspace("main", doc.clone()).unwrap();
        assert!(store.load_workspace("main-other").unwrap().is_none());
        drop(store);
        let store = crate::workspace::PlaceStore::new(directory.path()).unwrap();
        let restored = store.load_workspace("main").unwrap().unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), serde_json::to_value(doc).unwrap());
    }
    #[test]
    fn rejects_unknown_fields_including_credentials() {
        let mut json = serde_json::to_value(document()).unwrap();
        json["tabs"][0]["password"] = "must not persist".into();
        assert!(serde_json::from_value::<WorkspaceDocument>(json).is_err());
    }
    #[test]
    fn validates_version_ids_limits_and_layout() {
        let doc = document();
        doc.validate().unwrap();
        let mut invalid = doc.clone();
        invalid.version = 2;
        assert!(invalid.validate().is_err());
        let mut invalid = doc.clone();
        invalid.active_id = "missing".into();
        assert!(invalid.validate().is_err());
        let mut invalid = doc.clone();
        invalid.tabs.push(invalid.tabs[0].clone());
        assert!(invalid.validate().is_err());
        let mut invalid = doc.clone();
        invalid.tabs[0].ratio = f64::NAN;
        assert!(invalid.validate().is_err());
        let mut invalid = doc.clone();
        invalid.tabs[0].primary.path = "relative".into();
        assert!(invalid.validate().is_err());
        let mut invalid = doc.clone();
        invalid.tabs[0].widths.insert("password".into(), 120.0);
        assert!(invalid.validate().is_err());
        let mut invalid = doc;
        invalid.tabs[0].name = "x".repeat(81);
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn invalid_save_does_not_replace_previous_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::workspace::PlaceStore::new(directory.path()).unwrap();
        store.save_workspace("main", document()).unwrap();
        let mut invalid = document();
        invalid.tabs.clear();
        assert!(store.save_workspace("main", invalid).is_err());
        assert_eq!(store.load_workspace("main").unwrap().unwrap().tabs.len(), 1);
    }
    #[test]
    fn workspace_tab_limit_and_corrupt_document_are_not_silently_reset() {
        let mut doc = document();
        doc.tabs = (0..32)
            .map(|index| {
                let mut tab = doc.tabs[0].clone();
                tab.id = index.to_string();
                tab
            })
            .collect();
        doc.active_id = "0".into();
        doc.validate().unwrap();
        let mut extra = doc.tabs[0].clone();
        extra.id = "32".into();
        doc.tabs.push(extra);
        assert!(doc.validate().is_err());
        let directory = tempfile::tempdir().unwrap();
        let store = crate::workspace::PlaceStore::new(directory.path()).unwrap();
        store.save_workspace("main", document()).unwrap();
        let connection = rusqlite::Connection::open(directory.path().join("workspace.sqlite3")).unwrap();
        connection.execute("UPDATE workspaces SET document=?1 WHERE window='main'", ["{broken}"]).unwrap();
        assert!(store.load_workspace("main").is_err());
        let saved: String = connection
            .query_row("SELECT document FROM workspaces WHERE window='main'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(saved, "{broken}");
    }
}

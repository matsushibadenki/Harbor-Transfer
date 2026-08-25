use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncDirection {
    LocalToRemote,
    RemoteToLocal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub path: String,
    pub size: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyncAction {
    Upload,
    Download,
    CreateRemoteDirectory,
    CreateLocalDirectory,
    Conflict,
    DestinationOnly,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncPreviewItem {
    pub path: String,
    pub action: SyncAction,
    pub local_size: Option<u64>,
    pub remote_size: Option<u64>,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPreview {
    pub direction: SyncDirection,
    pub items: Vec<SyncPreviewItem>,
    pub transfer_count: usize,
    pub directory_count: usize,
    pub conflict_count: usize,
    pub destination_only_count: usize,
}

pub fn plan_sync(
    local_entries: Vec<SnapshotEntry>,
    remote_entries: Vec<SnapshotEntry>,
    direction: SyncDirection,
) -> SyncPreview {
    let local =
        local_entries.into_iter().map(|entry| (entry.path.clone(), entry)).collect::<BTreeMap<_, _>>();
    let remote =
        remote_entries.into_iter().map(|entry| (entry.path.clone(), entry)).collect::<BTreeMap<_, _>>();
    let mut paths = local.keys().chain(remote.keys()).cloned().collect::<Vec<_>>();
    paths.sort();
    paths.dedup();

    let mut items = Vec::new();
    for path in paths {
        let local_entry = local.get(&path);
        let remote_entry = remote.get(&path);
        let action = match (local_entry, remote_entry, direction) {
            (Some(local), None, SyncDirection::LocalToRemote) if local.is_directory => {
                SyncAction::CreateRemoteDirectory
            }
            (Some(_), None, SyncDirection::LocalToRemote) => SyncAction::Upload,
            (None, Some(remote), SyncDirection::RemoteToLocal) if remote.is_directory => {
                SyncAction::CreateLocalDirectory
            }
            (None, Some(_), SyncDirection::RemoteToLocal) => SyncAction::Download,
            (None, Some(_), SyncDirection::LocalToRemote) | (Some(_), None, SyncDirection::RemoteToLocal) => {
                SyncAction::DestinationOnly
            }
            (Some(local), Some(remote), _) if local.is_directory != remote.is_directory => {
                SyncAction::Conflict
            }
            (Some(local), Some(remote), _) if !local.is_directory && local.size != remote.size => {
                SyncAction::Conflict
            }
            _ => continue,
        };
        let is_directory = local_entry.or(remote_entry).is_some_and(|entry| entry.is_directory);
        items.push(SyncPreviewItem {
            path,
            action,
            local_size: local_entry.filter(|entry| !entry.is_directory).map(|entry| entry.size),
            remote_size: remote_entry.filter(|entry| !entry.is_directory).map(|entry| entry.size),
            is_directory,
        });
    }

    SyncPreview {
        direction,
        transfer_count: items
            .iter()
            .filter(|item| matches!(item.action, SyncAction::Upload | SyncAction::Download))
            .count(),
        directory_count: items
            .iter()
            .filter(|item| {
                matches!(item.action, SyncAction::CreateRemoteDirectory | SyncAction::CreateLocalDirectory)
            })
            .count(),
        conflict_count: items.iter().filter(|item| item.action == SyncAction::Conflict).count(),
        destination_only_count: items
            .iter()
            .filter(|item| item.action == SyncAction::DestinationOnly)
            .count(),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::{plan_sync, SnapshotEntry, SyncAction, SyncDirection};

    fn file(path: &str, size: u64) -> SnapshotEntry {
        SnapshotEntry { path: path.to_string(), size, is_directory: false }
    }

    #[test]
    fn previews_uploads_conflicts_and_destination_only_entries() {
        let preview = plan_sync(
            vec![file("new.txt", 10), file("changed.txt", 20)],
            vec![file("changed.txt", 12), file("remote.txt", 5)],
            SyncDirection::LocalToRemote,
        );
        assert_eq!(preview.transfer_count, 1);
        assert_eq!(preview.conflict_count, 1);
        assert_eq!(preview.destination_only_count, 1);
        assert_eq!(preview.items[0].action, SyncAction::Conflict);
        assert_eq!(preview.items[1].action, SyncAction::Upload);
    }

    #[test]
    fn omits_identical_files() {
        let preview =
            plan_sync(vec![file("same.txt", 10)], vec![file("same.txt", 10)], SyncDirection::RemoteToLocal);
        assert!(preview.items.is_empty());
    }
}

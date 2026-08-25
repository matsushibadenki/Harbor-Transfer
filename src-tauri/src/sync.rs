use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyncAction {
    Upload,
    Download,
    CreateRemoteDirectory,
    CreateLocalDirectory,
    Conflict,
    DestinationOnly,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
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

pub fn matches_exclusion(path: &str, patterns: &[String]) -> bool {
    let normalized_path = path.replace('\\', "/");
    patterns.iter().any(|raw_pattern| {
        let pattern = raw_pattern.trim().trim_start_matches("./").replace('\\', "/");
        if pattern.is_empty() {
            return false;
        }
        if pattern.contains('/') {
            let recursive_root = pattern.strip_suffix("/**");
            recursive_root == Some(normalized_path.as_str()) || glob_matches(&pattern, &normalized_path)
        } else {
            normalized_path.split('/').any(|component| glob_matches(&pattern, component))
        }
    })
}

pub fn filter_snapshot(entries: Vec<SnapshotEntry>, patterns: &[String]) -> Vec<SnapshotEntry> {
    entries.into_iter().filter(|entry| !matches_exclusion(&entry.path, patterns)).collect()
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.chars().collect::<Vec<_>>();
    let value = value.chars().collect::<Vec<_>>();
    let mut memo = BTreeMap::new();
    glob_matches_at(&pattern, &value, 0, 0, &mut memo)
}

fn glob_matches_at(
    pattern: &[char],
    value: &[char],
    pattern_index: usize,
    value_index: usize,
    memo: &mut BTreeMap<(usize, usize), bool>,
) -> bool {
    if let Some(result) = memo.get(&(pattern_index, value_index)) {
        return *result;
    }
    let result = if pattern_index == pattern.len() {
        value_index == value.len()
    } else if pattern[pattern_index] == '*' {
        let recursive = pattern.get(pattern_index + 1) == Some(&'*');
        let next_pattern = if recursive { pattern_index + 2 } else { pattern_index + 1 };
        glob_matches_at(pattern, value, next_pattern, value_index, memo)
            || (value_index < value.len()
                && (recursive || value[value_index] != '/')
                && glob_matches_at(pattern, value, pattern_index, value_index + 1, memo))
    } else if value_index < value.len()
        && ((pattern[pattern_index] == '?' && value[value_index] != '/')
            || pattern[pattern_index] == value[value_index])
    {
        glob_matches_at(pattern, value, pattern_index + 1, value_index + 1, memo)
    } else {
        false
    };
    memo.insert((pattern_index, value_index), result);
    result
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
    use super::{filter_snapshot, matches_exclusion, plan_sync, SnapshotEntry, SyncAction, SyncDirection};

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

    #[test]
    fn applies_component_and_recursive_exclusion_patterns() {
        let patterns = vec![".DS_Store".to_string(), "node_modules/**".to_string(), "*.tmp".to_string()];
        assert!(matches_exclusion("nested/.DS_Store", &patterns));
        assert!(matches_exclusion("node_modules", &patterns));
        assert!(matches_exclusion("node_modules/pkg/index.js", &patterns));
        assert!(matches_exclusion("cache/result.tmp", &patterns));
        assert!(!matches_exclusion("src/index.ts", &patterns));
        let filtered =
            filter_snapshot(vec![file("src/index.ts", 10), file("cache/result.tmp", 2)], &patterns);
        assert_eq!(filtered, vec![file("src/index.ts", 10)]);
    }
}

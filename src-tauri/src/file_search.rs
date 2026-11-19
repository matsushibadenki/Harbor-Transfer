//! Metadata-only traversal. Cancellation never drops an in-flight protocol operation.
use crate::{remote_fs::RemoteFileSystem, sftp_client::FileEntryType};
use async_trait::async_trait;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct SearchRegistry(Mutex<HashMap<String, (String, Arc<AtomicBool>)>>);
pub struct SearchLease<'a> {
    registry: &'a SearchRegistry,
    window: String,
    pub cancelled: Arc<AtomicBool>,
}
impl SearchRegistry {
    pub fn begin(&self, window: &str, id: &str) -> Result<SearchLease<'_>, String> {
        if id.is_empty() || id.len() > 128 {
            return Err("Invalid search ID.".into());
        }
        let mut jobs = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if jobs.contains_key(window) {
            return Err("A search is still running in this window.".into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        jobs.insert(window.into(), (id.into(), cancelled.clone()));
        Ok(SearchLease { registry: self, window: window.into(), cancelled })
    }
    pub fn cancel(&self, window: &str, id: Option<&str>) {
        if let Some((current, flag)) = self.0.lock().unwrap_or_else(|e| e.into_inner()).get(window) {
            if id.is_none_or(|id| id == current) {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}
impl Drop for SearchLease<'_> {
    fn drop(&mut self) {
        self.registry.0.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.window);
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub inspected: usize,
    pub directories: usize,
    pub truncated: bool,
    pub cancelled: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgress<'a> {
    pub search_id: &'a str,
    pub path: &'a str,
    pub inspected: usize,
    pub directories: usize,
    pub matches: usize,
}
pub struct SearchChild {
    pub name: String,
    pub component: String,
    pub directory: bool,
    pub symlink: bool,
}
#[async_trait]
pub trait SearchSource: Send {
    fn include_hidden(&self) -> bool {
        true
    }
    async fn list(&mut self, path: &str) -> Result<Vec<SearchChild>, String>;
}
pub struct VisibleSource<'a> {
    pub source: &'a mut dyn SearchSource,
    pub show_hidden: bool,
}
#[async_trait]
impl SearchSource for VisibleSource<'_> {
    fn include_hidden(&self) -> bool {
        self.show_hidden
    }
    async fn list(&mut self, path: &str) -> Result<Vec<SearchChild>, String> {
        self.source.list(path).await
    }
}
pub struct RemoteSource<'a>(pub &'a mut dyn RemoteFileSystem);
#[async_trait]
impl SearchSource for RemoteSource<'_> {
    async fn list(&mut self, path: &str) -> Result<Vec<SearchChild>, String> {
        Ok(self
            .0
            .list_dir(path)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|entry| SearchChild {
                component: entry.path_component.unwrap_or_else(|| entry.name.clone()),
                name: entry.name,
                directory: matches!(entry.file_type, FileEntryType::Directory),
                symlink: matches!(entry.file_type, FileEntryType::Symlink),
            })
            .collect())
    }
}
pub struct LocalSource;
#[async_trait]
impl SearchSource for LocalSource {
    async fn list(&mut self, path: &str) -> Result<Vec<SearchChild>, String> {
        if tokio::fs::symlink_metadata(path).await.map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("Search does not follow symbolic links.".into());
        }
        let mut directory = tokio::fs::read_dir(path).await.map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        while let Some(entry) = directory.next_entry().await.map_err(|e| e.to_string())? {
            let kind = entry.file_type().await.map_err(|e| e.to_string())?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            entries.push(SearchChild {
                component: name.clone(),
                name,
                directory: kind.is_dir(),
                symlink: kind.is_symlink(),
            });
            if entries.len() > 10000 {
                break;
            }
        }
        Ok(entries)
    }
}
fn component_valid(value: &str) -> bool {
    !value.is_empty() && value != "." && value != ".." && !value.contains('/') && !value.contains('\0')
}
pub fn validate(root: &str, query: &str) -> Result<(), String> {
    if query.trim().is_empty()
        || query.len() > 512
        || !root.starts_with('/')
        || root.len() > 16384
        || root.contains('\0')
        || root.split('/').any(|part| part == "..")
    {
        return Err("Invalid search query or root.".into());
    }
    Ok(())
}
pub async fn search(
    source: &mut dyn SearchSource,
    root: &str,
    query: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&SearchResult, &str) + Send,
) -> Result<SearchResult, String> {
    validate(root, query)?;
    let query = query.trim().to_lowercase();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut result = SearchResult::default();
    let mut pending = VecDeque::from([(root.to_string(), 0usize)]);
    let mut visited = HashSet::new();
    while let Some((directory, depth)) = pending.pop_front() {
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        if result.directories >= 256 || Instant::now() >= deadline {
            result.truncated = true;
            break;
        }
        if !visited.insert(directory.clone()) {
            continue;
        }
        progress(&result, &directory);
        // Finish the listing to keep FTP control channels synchronized.
        let children = match source.list(&directory).await {
            Ok(children) => children,
            Err(_) if cancel.load(Ordering::Relaxed) => {
                result.cancelled = true;
                break;
            }
            Err(error) => return Err(format!("Search could not list {directory}: {error}")),
        };
        result.directories += 1;
        for child in children {
            if cancel.load(Ordering::Relaxed) {
                result.cancelled = true;
                break;
            }
            if result.inspected >= 10000 || result.matches.len() >= 500 || Instant::now() >= deadline {
                result.truncated = true;
                progress(&result, &directory);
                return Ok(result);
            }
            result.inspected += 1;
            if !source.include_hidden() && child.name.starts_with('.') {
                continue;
            }
            if child.symlink || !component_valid(&child.component) {
                continue;
            }
            let path = format!("{}/{}", directory.trim_end_matches('/'), child.component);
            if path.len() > 16384 {
                result.truncated = true;
                continue;
            }
            if child.name.to_lowercase().contains(&query) {
                result.matches.push(SearchMatch {
                    name: child.name,
                    path: path.clone(),
                    is_directory: child.directory,
                });
            }
            if child.directory {
                if depth >= 32 || pending.len() >= 1024 {
                    result.truncated = true;
                } else {
                    pending.push_back((path, depth + 1));
                }
            }
            if result.inspected % 128 == 0 {
                progress(&result, &directory);
            }
        }
        progress(&result, &directory);
        if result.cancelled {
            break;
        }
        tokio::task::yield_now().await;
    }
    result.cancelled |= cancel.load(Ordering::Relaxed);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        visited: Vec<String>,
        cancel: Option<Arc<AtomicBool>>,
        many: bool,
    }
    #[async_trait]
    impl SearchSource for Fixture {
        async fn list(&mut self, path: &str) -> Result<Vec<SearchChild>, String> {
            self.visited.push(path.into());
            if let Some(cancel) = &self.cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            let entry = |name: String, component: String, directory, symlink| SearchChild {
                name,
                component,
                directory,
                symlink,
            };
            Ok(if self.many {
                (0..600).map(|i| entry(format!("file{i}"), i.to_string(), false, false)).collect()
            } else if path == "/" {
                vec![
                    entry("Folder".into(), "stable-id".into(), true, false),
                    entry("link".into(), "link".into(), true, true),
                    entry("escape".into(), "..".into(), true, false),
                ]
            } else {
                vec![entry("REPORT日本語.txt".into(), "opaque-id".into(), false, false)]
            })
        }
    }
    fn fixture() -> Fixture {
        Fixture { visited: vec![], cancel: None, many: false }
    }
    #[test]
    fn registry_window_ownership_and_validation() {
        for value in ["", ".", "..", "a/b", "a\0b"] {
            assert!(!component_valid(value));
        }
        assert!(validate("/../a", "name").is_err());
        assert!(validate("/", " ").is_err());
        let registry = SearchRegistry::default();
        let job = registry.begin("main", "a").unwrap();
        let other = registry.begin("second-window", "a").unwrap();
        assert!(registry.begin("main", "b").is_err());
        registry.cancel("other", Some("a"));
        registry.cancel("main", Some("wrong"));
        assert!(!job.cancelled.load(Ordering::Relaxed));
        registry.cancel("main", Some("a"));
        assert!(job.cancelled.load(Ordering::Relaxed));
        assert!(!other.cancelled.load(Ordering::Relaxed));
        drop(job);
        assert!(registry.begin("main", "b").is_ok());
    }
    #[tokio::test]
    async fn recursion_preserves_ids_and_reports_progress() {
        let mut source = fixture();
        let mut events = 0;
        let result = search(&mut source, "/", "report日本語", &AtomicBool::new(false), |_, _| events += 1)
            .await
            .unwrap();
        assert_eq!(result.matches[0].path, "/stable-id/opaque-id");
        assert_eq!(source.visited, ["/", "/stable-id"]);
        assert!(events >= 4);
    }
    #[tokio::test]
    async fn cancellation_stops_before_next_listing() {
        let cancel = Arc::new(AtomicBool::new(true));
        let mut source = fixture();
        assert!(search(&mut source, "/", "file", &cancel, |_, _| {}).await.unwrap().cancelled);
        assert!(source.visited.is_empty());
        cancel.store(false, Ordering::Relaxed);
        source.cancel = Some(cancel.clone());
        assert!(search(&mut source, "/", "file", &cancel, |_, _| {}).await.unwrap().cancelled);
        assert_eq!(source.visited, ["/"]);
    }
    #[tokio::test]
    async fn result_limit_is_explicit() {
        let mut source = fixture();
        source.many = true;
        let result = search(&mut source, "/", "file", &AtomicBool::new(false), |_, _| {}).await.unwrap();
        assert_eq!(result.matches.len(), 500);
        assert!(result.truncated);
    }
    #[tokio::test]
    async fn local_nested_file_and_symlink_loop() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("nested")).unwrap();
        std::fs::write(dir.path().join("nested/report.txt"), "test").unwrap();
        std::fs::create_dir(dir.path().join(".hidden")).unwrap();
        std::fs::write(dir.path().join(".hidden/report.txt"), "hidden").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path(), dir.path().join("loop")).unwrap();
        let result = search(
            &mut LocalSource,
            dir.path().to_str().unwrap(),
            "REPORT",
            &AtomicBool::new(false),
            |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(result.matches.len(), 2);
        assert_eq!(result.directories, 3);
        let result = search(
            &mut VisibleSource { source: &mut LocalSource, show_hidden: false },
            dir.path().to_str().unwrap(),
            "REPORT",
            &AtomicBool::new(false),
            |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.directories, 2);
    }
    struct BoundSource {
        chain: bool,
    }
    #[async_trait]
    impl SearchSource for BoundSource {
        async fn list(&mut self, _: &str) -> Result<Vec<SearchChild>, String> {
            Ok((0..if self.chain { 1 } else { 10001 })
                .map(|i| SearchChild {
                    name: format!("child{i}"),
                    component: format!("child{i}"),
                    directory: self.chain,
                    symlink: false,
                })
                .collect())
        }
    }
    #[tokio::test]
    async fn depth_and_item_limits_are_explicit() {
        let result =
            search(&mut BoundSource { chain: true }, "/", "no match", &AtomicBool::new(false), |_, _| {})
                .await
                .unwrap();
        assert_eq!(result.directories, 33);
        assert!(result.truncated);
        let result =
            search(&mut BoundSource { chain: false }, "/", "no match", &AtomicBool::new(false), |_, _| {})
                .await
                .unwrap();
        assert_eq!(result.inspected, 10000);
        assert!(result.truncated);
    }
    struct InFlight {
        cancel: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
    }
    #[async_trait]
    impl SearchSource for InFlight {
        async fn list(&mut self, _: &str) -> Result<Vec<SearchChild>, String> {
            self.cancel.store(true, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(10)).await;
            self.finished.store(true, Ordering::Relaxed);
            Ok(vec![])
        }
    }
    #[tokio::test]
    async fn cancellation_waits_for_in_flight_listing_to_complete() {
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let result = search(
            &mut InFlight { cancel: cancel.clone(), finished: finished.clone() },
            "/",
            "file",
            &cancel,
            |_, _| {},
        )
        .await
        .unwrap();
        assert!(result.cancelled);
        assert!(finished.load(Ordering::Relaxed));
    }
    #[tokio::test]
    async fn invalid_input_does_not_start_io() {
        let mut source = fixture();
        assert!(search(&mut source, "/", " ", &AtomicBool::new(false), |_, _| {}).await.is_err());
        assert!(source.visited.is_empty());
    }
}

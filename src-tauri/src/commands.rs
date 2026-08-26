use crate::bookmarks::{Bookmark, BookmarkStore, ConnectionHistory, SyncHistory, TransferHistory};
use crate::ftp_client::{FtpClient, FtpConfig};
use crate::remote_fs::RemoteFileSystem;
use crate::s3_client::{S3Client, S3Config};
use crate::sftp_client::{FileEntry, SftpAuthMethod, SftpConfig, StandaloneSftpClient};
use crate::ssh;
use crate::sync::{
    filter_snapshot, plan_sync_with_comparison, SnapshotEntry, SyncAction, SyncComparison, SyncDirection,
    SyncPreview,
};
use crate::webdav_client::{WebDavClient, WebDavConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, State};
use tokio::sync::Mutex;

static EDIT_SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static DRAG_EXPORT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static REMOTE_COPY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub struct AppState {
    connections: Mutex<HashMap<String, RemoteConnection>>,
    bookmarks: BookmarkStore,
    transfer_controls: Mutex<HashMap<String, Arc<TransferControl>>>,
    edit_cache_directory: PathBuf,
    remote_edits: Mutex<HashMap<String, RemoteEditSession>>,
    drag_cache_directory: PathBuf,
    drag_icon_path: PathBuf,
    drag_exports: Mutex<HashMap<String, PathBuf>>,
}

impl AppState {
    pub fn new(data_directory: PathBuf, cache_directory: PathBuf) -> Result<Self, String> {
        let edit_cache_directory = cache_directory.join("remote-edit");
        std::fs::create_dir_all(&edit_cache_directory).map_err(|error| error.to_string())?;
        let drag_cache_directory = cache_directory.join("drag-export");
        std::fs::create_dir_all(&drag_cache_directory).map_err(|error| error.to_string())?;
        let drag_icon_path = drag_cache_directory.join("drag-preview.png");
        std::fs::write(&drag_icon_path, include_bytes!("../icons/128x128.png"))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            connections: Mutex::new(HashMap::new()),
            bookmarks: BookmarkStore::new(&data_directory)?,
            transfer_controls: Mutex::new(HashMap::new()),
            edit_cache_directory,
            remote_edits: Mutex::new(HashMap::new()),
            drag_cache_directory,
            drag_icon_path,
            drag_exports: Mutex::new(HashMap::new()),
        })
    }
}

#[derive(Clone)]
struct RemoteEditSession {
    connection_id: String,
    remote_path: String,
    cache_file: PathBuf,
    cache_directory: PathBuf,
    uploaded_hash: u64,
    pending_hash: Option<u64>,
    pending_since: Option<Instant>,
    uploading: bool,
}

struct TransferControl {
    paused: AtomicBool,
    cancelled: AtomicBool,
    resumed: tokio::sync::Notify,
}

impl TransferControl {
    fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            resumed: tokio::sync::Notify::new(),
        }
    }
    async fn wait_until_running(&self) -> Result<(), String> {
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err("Transfer cancelled.".to_string());
            }
            if !self.paused.load(Ordering::Acquire) {
                return Ok(());
            }
            self.resumed.notified().await;
        }
    }
}

enum RemoteConnection {
    Sftp(StandaloneSftpClient),
    Ftp { client: FtpClient, protocol: Protocol },
    WebDav(WebDavClient),
    S3(S3Client),
}

impl RemoteConnection {
    fn file_system(&mut self) -> &mut dyn RemoteFileSystem {
        match self {
            Self::Sftp(client) => client,
            Self::Ftp { client, .. } => client,
            Self::WebDav(client) => client,
            Self::S3(client) => client,
        }
    }

    fn protocol(&self) -> Protocol {
        match self {
            Self::Sftp(_) => Protocol::Sftp,
            Self::Ftp { protocol, .. } => *protocol,
            Self::WebDav(_) => Protocol::Webdav,
            Self::S3(_) => Protocol::S3,
        }
    }
}

// Do not derive `Debug`: the request carries passwords and S3 credentials.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub connection_id: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub passphrase: Option<String>,
    pub expected_host_key: Option<String>,
    pub initial_path: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_session_token: Option<String>,
    #[serde(default)]
    pub s3_force_path_style: bool,
    #[serde(default)]
    pub s3_preserve_empty_directories: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Sftp,
    Ftp,
    Ftps,
    Webdav,
    S3,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSummary {
    pub connection_id: String,
    pub protocol: Protocol,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePathRequest {
    pub connection_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRequest {
    pub connection_id: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRequest {
    pub connection_id: String,
    pub path: String,
    pub is_directory: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePasteRequest {
    pub connection_id: String,
    pub source_path: String,
    pub destination_path: String,
    pub is_directory: bool,
    pub cut: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetMetadataRequest {
    pub connection_id: String,
    pub path: String,
    pub permissions: Option<u32>,
    pub modified: Option<u64>,
    pub owner_id: Option<u32>,
    pub group_id: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRequest {
    pub transfer_id: Option<String>,
    pub connection_id: String,
    pub local_path: String,
    pub remote_path: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileTransferProgress {
    pub transfer_id: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub elapsed_ms: u64,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPathInfo {
    pub name: String,
    pub is_directory: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEditOpenRequest {
    pub connection_id: String,
    pub remote_path: String,
    pub editor_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEditOpenResult {
    pub edit_id: String,
    pub name: String,
    pub remote_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEditPollResult {
    pub edit_id: String,
    pub remote_path: String,
    pub status: String,
    pub bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DragExportPrepareRequest {
    pub connection_id: String,
    pub remote_path: String,
    pub is_directory: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragExportPrepareResult {
    pub export_id: String,
    pub name: String,
    pub remote_path: String,
    pub local_path: String,
    pub icon_path: String,
}

enum RemoteExportEntry {
    Directory(PathBuf),
    File(PathBuf),
}

async fn collect_remote_export_entries(
    file_system: &mut dyn RemoteFileSystem,
    root: &str,
) -> Result<Vec<RemoteExportEntry>, String> {
    let mut result = Vec::new();
    let mut directories = vec![(root.to_string(), PathBuf::new())];
    while let Some((remote_directory, relative_directory)) = directories.pop() {
        let entries = file_system.list_dir(&remote_directory).await.map_err(|error| error.to_string())?;
        for entry in entries {
            let relative = relative_directory.join(&entry.name);
            let relative_text = relative.to_string_lossy();
            safe_relative_path(&relative_text)?;
            match entry.file_type {
                crate::sftp_client::FileEntryType::Directory => {
                    if result.len() >= 100_000 {
                        return Err("The folder contains too many items to drag safely.".to_string());
                    }
                    let child = remote_join(root, &relative);
                    result.push(RemoteExportEntry::Directory(relative.clone()));
                    directories.push((child, relative));
                }
                crate::sftp_client::FileEntryType::File => {
                    if result.len() >= 100_000 {
                        return Err("The folder contains too many items to drag safely.".to_string());
                    }
                    result.push(RemoteExportEntry::File(relative));
                }
                crate::sftp_client::FileEntryType::Symlink => {
                    return Err(format!(
                        "Folder drag does not follow symbolic links: {}",
                        relative.display()
                    ));
                }
            }
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn drag_export_prepare(
    request: DragExportPrepareRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<DragExportPrepareResult, String> {
    if request.connection_id.trim().is_empty() || request.remote_path.trim().is_empty() {
        return Err("Invalid drag export request.".to_string());
    }
    let name = Path::new(&request.remote_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or("The remote file name is invalid.")?
        .to_string();
    let sequence = DRAG_EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp =
        SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_millis();
    let export_id = format!("drag-{timestamp}-{sequence}");
    let export_directory = state.drag_cache_directory.join(&export_id);
    std::fs::create_dir(&export_directory).map_err(|error| error.to_string())?;
    let local_item = export_directory.join(&name);
    let prepare = {
        let mut connections = state.connections.lock().await;
        match connections.get_mut(&request.connection_id) {
            Some(connection) if request.is_directory => {
                let file_system = connection.file_system();
                match collect_remote_export_entries(file_system, &request.remote_path).await {
                    Ok(entries) => {
                        let mut outcome = std::fs::create_dir(&local_item).map_err(|error| error.to_string());
                        for entry in entries {
                            if outcome.is_err() {
                                break;
                            }
                            outcome = match entry {
                                RemoteExportEntry::Directory(relative) => {
                                    std::fs::create_dir_all(local_item.join(relative))
                                        .map_err(|error| error.to_string())
                                }
                                RemoteExportEntry::File(relative) => {
                                    let local_file = local_item.join(&relative);
                                    let parent_result = local_file
                                        .parent()
                                        .ok_or("Invalid drag cache path.".to_string())
                                        .and_then(|parent| {
                                            std::fs::create_dir_all(parent).map_err(|error| error.to_string())
                                        });
                                    match parent_result {
                                        Ok(()) => file_system
                                            .download_file(
                                                &remote_join(&request.remote_path, &relative),
                                                &local_file.to_string_lossy(),
                                            )
                                            .await
                                            .map(|_| ())
                                            .map_err(|error| error.to_string()),
                                        Err(error) => Err(error),
                                    }
                                }
                            };
                        }
                        outcome
                    }
                    Err(error) => Err(error),
                }
            }
            Some(connection) => connection
                .file_system()
                .download_file(&request.remote_path, &local_item.to_string_lossy())
                .await
                .map(|_| ())
                .map_err(|error| error.to_string()),
            None => Err("Connection not found.".to_string()),
        }
    };
    if let Err(error) = prepare {
        let _ = std::fs::remove_dir_all(&export_directory);
        return Err(error);
    }
    let metadata = std::fs::symlink_metadata(&local_item).map_err(|error| error.to_string())?;
    let expected_type =
        if request.is_directory { metadata.file_type().is_dir() } else { metadata.file_type().is_file() };
    if !expected_type || metadata.file_type().is_symlink() {
        let _ = std::fs::remove_dir_all(&export_directory);
        return Err("The drag cache has an unexpected item type.".to_string());
    }
    state.drag_exports.lock().await.insert(export_id.clone(), export_directory);
    Ok(DragExportPrepareResult {
        export_id,
        name,
        remote_path: request.remote_path,
        local_path: local_item.to_string_lossy().to_string(),
        icon_path: state.drag_icon_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn drag_export_cleanup(
    export_id: String,
    delay_ms: Option<u64>,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let directory = state.drag_exports.lock().await.remove(&export_id);
    if let Some(directory) = directory {
        let delay_ms = delay_ms.unwrap_or(0).min(60 * 60 * 1000);
        if delay_ms == 0 {
            std::fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
        } else {
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                let _ = std::fs::remove_dir_all(directory);
            });
        }
    }
    Ok(())
}

fn content_hash(path: &Path) -> Result<u64, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("The editing cache is no longer a regular file.".to_string());
    }
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = DefaultHasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        buffer[..read].hash(&mut hasher);
    }
    Ok(hasher.finish())
}

fn launch_editor(editor_path: &Path, cache_file: &Path) -> Result<(), String> {
    let editor = editor_path.canonicalize().map_err(|error| error.to_string())?;
    let is_app_bundle =
        editor.extension().and_then(|extension| extension.to_str()) == Some("app") && editor.is_dir();
    let mut command = if is_app_bundle {
        let mut command = std::process::Command::new("/usr/bin/open");
        command.arg("-a").arg(&editor);
        command
    } else if editor.is_file() {
        std::process::Command::new(&editor)
    } else {
        return Err("The selected editor is not an application or executable file.".to_string());
    };
    command.arg(cache_file).spawn().map(|_| ()).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remote_edit_open(
    request: RemoteEditOpenRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<RemoteEditOpenResult, String> {
    if request.connection_id.trim().is_empty() || request.remote_path.trim().is_empty() {
        return Err("Invalid remote edit request.".to_string());
    }
    let name = Path::new(&request.remote_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or("The remote file name is invalid.")?
        .to_string();
    let sequence = EDIT_SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp =
        SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_millis();
    let edit_id = format!("edit-{timestamp}-{sequence}");
    let cache_directory = state.edit_cache_directory.join(&edit_id);
    std::fs::create_dir(&cache_directory).map_err(|error| error.to_string())?;
    let cache_file = cache_directory.join(&name);

    let download = {
        let mut connections = state.connections.lock().await;
        match connections.get_mut(&request.connection_id) {
            Some(connection) => connection
                .file_system()
                .download_file(&request.remote_path, &cache_file.to_string_lossy())
                .await
                .map_err(|error| error.to_string()),
            None => Err("Connection not found.".to_string()),
        }
    };
    if let Err(error) = download {
        let _ = std::fs::remove_dir_all(&cache_directory);
        return Err(error);
    }
    let uploaded_hash = match content_hash(&cache_file) {
        Ok(hash) => hash,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&cache_directory);
            return Err(error);
        }
    };
    if let Err(error) = launch_editor(Path::new(&request.editor_path), &cache_file) {
        let _ = std::fs::remove_dir_all(&cache_directory);
        return Err(error);
    }
    state.remote_edits.lock().await.insert(
        edit_id.clone(),
        RemoteEditSession {
            connection_id: request.connection_id,
            remote_path: request.remote_path.clone(),
            cache_file,
            cache_directory,
            uploaded_hash,
            pending_hash: None,
            pending_since: None,
            uploading: false,
        },
    );
    Ok(RemoteEditOpenResult { edit_id, name, remote_path: request.remote_path })
}

#[tauri::command]
pub async fn remote_edit_reopen(
    edit_id: String,
    editor_path: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let cache_file = state
        .remote_edits
        .lock()
        .await
        .get(&edit_id)
        .map(|session| session.cache_file.clone())
        .ok_or("Edit session not found.")?;
    // Validate that the watched cache has not been replaced with a symlink or
    // another unsafe file type before handing it to an external application.
    content_hash(&cache_file)?;
    launch_editor(Path::new(&editor_path), &cache_file)
}

#[tauri::command]
pub async fn remote_edit_poll(
    edit_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<RemoteEditPollResult, String> {
    let snapshot = state.remote_edits.lock().await.get(&edit_id).cloned().ok_or("Edit session not found.")?;
    let current_hash = content_hash(&snapshot.cache_file)?;
    let should_upload = {
        let mut edits = state.remote_edits.lock().await;
        let edit = edits.get_mut(&edit_id).ok_or("Edit session not found.")?;
        if edit.uploading {
            false
        } else if current_hash == edit.uploaded_hash {
            edit.pending_hash = None;
            edit.pending_since = None;
            return Ok(RemoteEditPollResult {
                edit_id,
                remote_path: edit.remote_path.clone(),
                status: "clean".to_string(),
                bytes: 0,
            });
        } else if edit.pending_hash != Some(current_hash) {
            edit.pending_hash = Some(current_hash);
            edit.pending_since = Some(Instant::now());
            false
        } else if edit.pending_since.is_some_and(|started| started.elapsed() >= Duration::from_millis(750)) {
            edit.uploading = true;
            true
        } else {
            false
        }
    };
    if !should_upload {
        return Ok(RemoteEditPollResult {
            edit_id,
            remote_path: snapshot.remote_path,
            status: "waiting".to_string(),
            bytes: 0,
        });
    }

    let upload = {
        let mut connections = state.connections.lock().await;
        match connections.get_mut(&snapshot.connection_id) {
            Some(connection) => connection
                .file_system()
                .upload_file(&snapshot.cache_file.to_string_lossy(), &snapshot.remote_path)
                .await
                .map_err(|error| error.to_string()),
            None => Err("Connection not found.".to_string()),
        }
    };
    let mut edits = state.remote_edits.lock().await;
    let edit = edits.get_mut(&edit_id).ok_or("Edit session not found.")?;
    edit.uploading = false;
    match upload {
        Ok(bytes) => {
            edit.uploaded_hash = current_hash;
            edit.pending_hash = None;
            edit.pending_since = None;
            Ok(RemoteEditPollResult {
                edit_id,
                remote_path: edit.remote_path.clone(),
                status: "uploaded".to_string(),
                bytes,
            })
        }
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub async fn remote_edit_close(edit_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut edits = state.remote_edits.lock().await;
    let current = edits.get(&edit_id).ok_or("Edit session not found.")?;
    if current.uploading {
        return Err("Wait for the edited file to finish uploading.".to_string());
    }
    if content_hash(&current.cache_file)? != current.uploaded_hash {
        return Err("The editing cache still has changes waiting to upload.".to_string());
    }
    let session = edits.remove(&edit_id).ok_or("Edit session not found.")?;
    drop(edits);
    std::fs::remove_dir_all(session.cache_directory).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn local_path_info(path: String) -> Result<LocalPathInfo, String> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
    Ok(LocalPathInfo {
        name: path.file_name().and_then(|name| name.to_str()).unwrap_or("item").to_string(),
        is_directory: metadata.is_dir(),
    })
}

fn credential_entry(bookmark_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("Harbor Transfer", bookmark_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn credential_load(bookmark_id: String) -> Result<Option<String>, String> {
    match credential_entry(&bookmark_id)?.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub async fn credential_save(bookmark_id: String, password: String) -> Result<(), String> {
    if password.is_empty() {
        return Ok(());
    }
    credential_entry(&bookmark_id)?.set_password(&password).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn credential_delete(bookmark_id: String) -> Result<(), String> {
    match credential_entry(&bookmark_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryTransferRequest {
    pub transfer_id: String,
    pub connection_id: String,
    pub local_directory: String,
    pub remote_directory: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryTransferProgress {
    pub transfer_id: String,
    pub completed_files: usize,
    pub total_files: usize,
    pub current_path: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPreviewRequest {
    pub connection_id: String,
    pub local_directory: String,
    pub remote_directory: String,
    pub direction: SyncDirection,
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default)]
    pub comparison: SyncComparison,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncExecutionSelection {
    pub path: String,
    pub action: SyncAction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncExecuteRequest {
    pub sync_id: String,
    pub connection_id: String,
    pub local_directory: String,
    pub remote_directory: String,
    pub direction: SyncDirection,
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default)]
    pub comparison: SyncComparison,
    pub items: Vec<SyncExecutionSelection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncExecutionLogItem {
    pub path: String,
    pub action: SyncAction,
    pub status: String,
    pub detail: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncExecutionResult {
    pub sync_id: String,
    pub status: String,
    pub completed_items: usize,
    pub total_items: usize,
    pub bytes: u64,
    pub log: Vec<SyncExecutionLogItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncExecutionProgress {
    pub sync_id: String,
    pub completed_items: usize,
    pub total_items: usize,
    pub current_path: String,
    pub status: String,
}

#[tauri::command]
pub async fn transfer_pause(transfer_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let controls = state.transfer_controls.lock().await;
    let control = controls.get(&transfer_id).ok_or("Transfer not found.")?;
    control.paused.store(true, Ordering::Release);
    Ok(())
}

#[tauri::command]
pub async fn transfer_resume(transfer_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let controls = state.transfer_controls.lock().await;
    let control = controls.get(&transfer_id).ok_or("Transfer not found.")?;
    control.paused.store(false, Ordering::Release);
    control.resumed.notify_waiters();
    Ok(())
}

#[tauri::command]
pub async fn transfer_cancel(transfer_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let controls = state.transfer_controls.lock().await;
    let control = controls.get(&transfer_id).ok_or("Transfer not found.")?;
    control.cancelled.store(true, Ordering::Release);
    control.resumed.notify_waiters();
    Ok(())
}

enum LocalEntry {
    Directory(PathBuf),
    File(PathBuf, PathBuf),
}

fn collect_local_entries(root: &Path, current: &Path, entries: &mut Vec<LocalEntry>) -> Result<(), String> {
    for entry in std::fs::read_dir(current).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let relative_path = path.strip_prefix(root).map_err(|error| error.to_string())?.to_path_buf();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            entries.push(LocalEntry::Directory(relative_path));
            collect_local_entries(root, &path, entries)?;
        } else if file_type.is_file() {
            entries.push(LocalEntry::File(path, relative_path));
        }
    }
    Ok(())
}

fn remote_join(base: &str, relative: &Path) -> String {
    let relative = relative.to_string_lossy().replace('\\', "/");
    format!("{}/{}", base.trim_end_matches('/'), relative.trim_start_matches('/'))
}

fn remote_child_path(base: &str, name: &str) -> Result<String, String> {
    let mut components = Path::new(name).components();
    if name.is_empty()
        || name.contains('/')
        || name.contains('\0')
        || !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("The server returned an unsafe child path while replacing the folder.".to_string());
    }
    Ok(remote_join(base, Path::new(name)))
}

fn remote_replace_target(path: &str) -> Result<String, String> {
    let target = path.trim_end_matches('/');
    if !target.starts_with('/')
        || target.is_empty()
        || target.split('/').any(|component| component == "." || component == "..")
    {
        return Err("Refusing to replace an unsafe remote path or the remote root directory.".to_string());
    }
    Ok(target.to_string())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.is_empty() || value.contains('\0') {
        return Err("Invalid empty sync path.".to_string());
    }
    let path = PathBuf::from(value);
    if path.components().any(|component| !matches!(component, std::path::Component::Normal(_))) {
        return Err(format!("Unsafe sync path: {value}"));
    }
    Ok(path)
}

fn reject_symlink_ancestors(root: &Path, target: &Path) -> Result<(), String> {
    let relative = target.strip_prefix(root).map_err(|_| "Sync path escaped the local root.".to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!("Refusing to follow a symlink during sync: {}", current.display()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn direction_label(direction: SyncDirection) -> &'static str {
    match direction {
        SyncDirection::LocalToRemote => "localToRemote",
        SyncDirection::RemoteToLocal => "remoteToLocal",
    }
}

fn parse_remote_modified(value: &str) -> Option<i64> {
    if let Ok(seconds) = value.trim().parse::<i64>() {
        return Some(seconds);
    }
    let value = value.trim();
    if value.len() >= 19 && value.as_bytes().get(4) == Some(&b'-') {
        return parse_date_time_components(&value[..19].replace('T', " "));
    }
    let fields = value.split_whitespace().collect::<Vec<_>>();
    if fields.len() >= 6 && fields[0].ends_with(',') {
        let month = match fields[2] {
            "Jan" => 1,
            "Feb" => 2,
            "Mar" => 3,
            "Apr" => 4,
            "May" => 5,
            "Jun" => 6,
            "Jul" => 7,
            "Aug" => 8,
            "Sep" => 9,
            "Oct" => 10,
            "Nov" => 11,
            "Dec" => 12,
            _ => return None,
        };
        let day = fields[1].parse().ok()?;
        let year = fields[3].parse().ok()?;
        let time = fields[4].split(':').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>().ok()?;
        if time.len() != 3 {
            return None;
        }
        return unix_seconds_from_components(year, month, day, time[0], time[1], time[2]);
    }
    None
}

fn parse_date_time_components(value: &str) -> Option<i64> {
    let date_time = value.split_once(' ')?;
    let date = date_time.0.split('-').map(str::parse::<i32>).collect::<Result<Vec<_>, _>>().ok()?;
    let time = date_time.1.split(':').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>().ok()?;
    if date.len() != 3 || time.len() != 3 {
        return None;
    }
    unix_seconds_from_components(date[0], date[1] as u32, date[2] as u32, time[0], time[1], time[2])
}

fn unix_seconds_from_components(
    mut year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    year -= i32::from(month <= 2);
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let year_of_era = year - era * 400;
    let adjusted_month = month as i32 + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era as i64 * 146_097 + day_of_era as i64 - 719_468;
    Some(days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64)
}

fn collect_local_snapshot(root: &Path) -> Result<Vec<SnapshotEntry>, String> {
    if !root.is_dir() {
        return Err("The selected local path is not a directory.".to_string());
    }
    let mut snapshot = Vec::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let is_directory = file_type.is_dir();
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            let size = if is_directory { 0 } else { metadata.len() };
            let modified = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_secs() as i64);
            snapshot.push(SnapshotEntry { path: relative, size, is_directory, modified });
            if is_directory {
                directories.push(path);
            }
        }
    }
    Ok(snapshot)
}

async fn collect_remote_snapshot(
    file_system: &mut dyn RemoteFileSystem,
    root: &str,
) -> Result<Vec<SnapshotEntry>, String> {
    let mut snapshot = Vec::new();
    let mut directories = vec![(root.to_string(), String::new())];
    while let Some((remote_directory, relative_directory)) = directories.pop() {
        let entries = file_system.list_dir(&remote_directory).await.map_err(|error| error.to_string())?;
        for entry in entries {
            let relative = if relative_directory.is_empty() {
                entry.name.clone()
            } else {
                format!("{relative_directory}/{}", entry.name)
            };
            let is_directory = matches!(entry.file_type, crate::sftp_client::FileEntryType::Directory);
            snapshot.push(SnapshotEntry {
                path: relative.clone(),
                size: entry.size,
                is_directory,
                modified: entry.modified.as_deref().and_then(parse_remote_modified),
            });
            if is_directory {
                let child = format!("{}/{}", remote_directory.trim_end_matches('/'), entry.name);
                directories.push((child, relative));
            }
        }
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn bookmarks_list(state: State<'_, Arc<AppState>>) -> Result<Vec<Bookmark>, String> {
    state.bookmarks.list()
}

#[tauri::command]
pub async fn bookmark_save(bookmark: Bookmark, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.bookmarks.save(&bookmark)
}

#[tauri::command]
pub async fn bookmark_delete(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.bookmarks.delete(&id)
}

#[tauri::command]
pub async fn connection_history_list(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ConnectionHistory>, String> {
    state.bookmarks.history()
}

#[tauri::command]
pub async fn connection_history_record(
    bookmark: Bookmark,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.bookmarks.record_history(&bookmark)
}

#[tauri::command]
pub async fn connection_history_clear(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.bookmarks.clear_history()
}

#[tauri::command]
pub async fn transfer_history_list(state: State<'_, Arc<AppState>>) -> Result<Vec<TransferHistory>, String> {
    state.bookmarks.transfer_history()
}

#[tauri::command]
pub async fn transfer_history_record(
    transfer: TransferHistory,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.bookmarks.record_transfer(&transfer)
}

#[tauri::command]
pub async fn transfer_history_clear(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.bookmarks.clear_transfer_history()
}

#[tauri::command]
pub async fn sync_history_list(state: State<'_, Arc<AppState>>) -> Result<Vec<SyncHistory>, String> {
    state.bookmarks.sync_history()
}

#[tauri::command]
pub async fn sync_history_clear(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.bookmarks.clear_sync_history()
}

/// Metadata only: private-key bytes never cross the Tauri IPC boundary.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeySummary {
    pub name: String,
    pub path: String,
    pub public_key_path: Option<String>,
    pub paired_key_path: Option<String>,
    pub key_type: String,
    pub kind: String,
}

#[tauri::command]
pub async fn connection_connect(
    request: ConnectRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<ConnectionSummary, String> {
    let protocol = request.protocol;
    let connection = match protocol {
        Protocol::Sftp => {
            let auth_method = match request.key_path.filter(|value| !value.trim().is_empty()) {
                Some(key_path) => SftpAuthMethod::PublicKey { key_path, passphrase: request.passphrase },
                None => SftpAuthMethod::Password {
                    password: request.password.ok_or("A password or SSH key is required.")?,
                },
            };
            RemoteConnection::Sftp(
                StandaloneSftpClient::connect(&SftpConfig {
                    host: request.host,
                    port: request.port,
                    username: request.username,
                    auth_method,
                    expected_host_key: request.expected_host_key,
                })
                .await
                .map_err(|error| error.to_string())?,
            )
        }
        Protocol::Ftp | Protocol::Ftps => RemoteConnection::Ftp {
            client: FtpClient::connect(&FtpConfig {
                host: request.host,
                port: request.port,
                username: request.username,
                password: request.password.unwrap_or_default(),
                ftps_enabled: matches!(protocol, Protocol::Ftps),
                anonymous: false,
            })
            .await
            .map_err(|error| error.to_string())?,
            protocol,
        },
        Protocol::Webdav => RemoteConnection::WebDav(
            WebDavClient::connect(&WebDavConfig {
                host: request.host,
                port: request.port,
                username: request.username,
                password: request.password.unwrap_or_default(),
                probe_path: request.initial_path.unwrap_or_else(|| "/".to_string()),
            })
            .await
            .map_err(|error| error.to_string())?,
        ),
        Protocol::S3 => RemoteConnection::S3(
            S3Client::connect(&S3Config {
                region: request.s3_region.ok_or("An S3 region is required.")?,
                bucket: request.host,
                endpoint: request.s3_endpoint.filter(|value| !value.trim().is_empty()),
                access_key_id: request.username,
                secret_access_key: request.password.ok_or("An S3 Secret Access Key is required.")?,
                session_token: request.s3_session_token.filter(|value| !value.trim().is_empty()),
                force_path_style: request.s3_force_path_style,
                preserve_empty_directories: request.s3_preserve_empty_directories,
                probe_path: request.initial_path.unwrap_or_else(|| "/".to_string()),
            })
            .await
            .map_err(|error| error.to_string())?,
        ),
    };

    state.connections.lock().await.insert(request.connection_id.clone(), connection);
    Ok(ConnectionSummary { connection_id: request.connection_id, protocol })
}

#[tauri::command]
pub async fn sftp_probe_host_key(host: String, port: u16) -> Result<String, String> {
    ssh::probe_host_key(&host, port).await
}

#[tauri::command]
pub async fn connection_disconnect(
    connection_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let connection = state.connections.lock().await.remove(&connection_id);
    let mut connection = connection.ok_or("Connection not found.".to_string())?;
    connection.file_system().disconnect().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn connection_list(state: State<'_, Arc<AppState>>) -> Result<Vec<ConnectionSummary>, String> {
    Ok(state
        .connections
        .lock()
        .await
        .iter()
        .map(|(connection_id, connection)| ConnectionSummary {
            connection_id: connection_id.clone(),
            protocol: connection.protocol(),
        })
        .collect())
}

#[tauri::command]
pub async fn ssh_keys_list() -> Result<Vec<SshKeySummary>, String> {
    let home = std::env::var_os("HOME").ok_or("Home directory is unavailable.")?;
    let ssh_directory = std::path::PathBuf::from(home).join(".ssh");
    if !ssh_directory.exists() {
        return Ok(Vec::new());
    }

    let ignored_names = ["config", "known_hosts", "authorized_keys", "environment"];
    let mut keys = std::fs::read_dir(&ssh_directory)
        .map_err(|error| format!("Failed to read ~/.ssh: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type().ok()?.is_file()
                || name.starts_with('.')
                || ignored_names.contains(&name.as_str())
            {
                return None;
            }
            let is_public = name.ends_with(".pub");
            let private_name = name.strip_suffix(".pub").unwrap_or(&name);
            let private_key_path = path.with_file_name(private_name);
            let public_key_path = path.with_file_name(format!("{name}.pub"));
            Some(SshKeySummary {
                kind: if private_name.starts_with("id_") {
                    "OpenSSH".to_string()
                } else {
                    "SSH key".to_string()
                },
                name,
                path: path.to_string_lossy().to_string(),
                public_key_path: (!is_public && public_key_path.is_file())
                    .then(|| public_key_path.to_string_lossy().to_string()),
                paired_key_path: if is_public && private_key_path.is_file() {
                    Some(private_key_path.to_string_lossy().to_string())
                } else if !is_public && public_key_path.is_file() {
                    Some(public_key_path.to_string_lossy().to_string())
                } else {
                    None
                },
                key_type: if is_public { "public".to_string() } else { "private".to_string() },
            })
        })
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| {
        left.name
            .trim_end_matches(".pub")
            .to_lowercase()
            .cmp(&right.name.trim_end_matches(".pub").to_lowercase())
            .then_with(|| left.key_type.cmp(&right.key_type))
    });
    Ok(keys)
}

#[tauri::command]
pub async fn remote_list(
    request: RemotePathRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<FileEntry>, String> {
    let mut connections = state.connections.lock().await;
    let connection = connections.get_mut(&request.connection_id).ok_or("Connection not found.")?;
    connection.file_system().list_dir(&request.path).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_preview(
    request: SyncPreviewRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncPreview, String> {
    let local =
        filter_snapshot(collect_local_snapshot(Path::new(&request.local_directory))?, &request.exclusions);
    let mut connections = state.connections.lock().await;
    let connection = connections.get_mut(&request.connection_id).ok_or("Connection not found.")?;
    let remote = filter_snapshot(
        collect_remote_snapshot(connection.file_system(), &request.remote_directory).await?,
        &request.exclusions,
    );
    Ok(plan_sync_with_comparison(local, remote, request.direction, request.comparison))
}

#[tauri::command]
pub async fn sync_execute(
    request: SyncExecuteRequest,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncExecutionResult, String> {
    if request.sync_id.trim().is_empty() || request.items.len() > 100_000 {
        return Err("Invalid sync execution request.".to_string());
    }
    let local_root = PathBuf::from(&request.local_directory);
    let canonical_local_root = local_root.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_local_root.is_dir() {
        return Err("The selected local path is not a directory.".to_string());
    }

    let local = filter_snapshot(collect_local_snapshot(&canonical_local_root)?, &request.exclusions);
    let mut connections = state.connections.lock().await;
    let connection = connections.get_mut(&request.connection_id).ok_or("Connection not found.")?;
    let remote = filter_snapshot(
        collect_remote_snapshot(connection.file_system(), &request.remote_directory).await?,
        &request.exclusions,
    );
    let current_plan = plan_sync_with_comparison(local, remote, request.direction, request.comparison);
    let planned =
        current_plan.items.into_iter().map(|item| (item.path.clone(), item)).collect::<HashMap<_, _>>();

    let mut selected = HashMap::new();
    for item in request.items {
        safe_relative_path(&item.path)?;
        let current = planned
            .get(&item.path)
            .ok_or_else(|| format!("The sync plan changed for {}. Refresh the preview.", item.path))?;
        let conflict_source_action = match request.direction {
            SyncDirection::LocalToRemote => SyncAction::Upload,
            SyncDirection::RemoteToLocal => SyncAction::Download,
        };
        let allowed = current.action == item.action
            || (current.action == SyncAction::Conflict
                && !current.is_directory
                && item.action == conflict_source_action);
        if !allowed
            || matches!(item.action, SyncAction::Conflict | SyncAction::DestinationOnly)
            || selected.insert(item.path.clone(), item.action).is_some()
        {
            return Err(format!("Unsafe or stale sync action for {}. Refresh the preview.", item.path));
        }
    }

    let mut items = selected.into_iter().collect::<Vec<_>>();
    items.sort_by(|(left_path, left_action), (right_path, right_action)| {
        let left_file = matches!(left_action, SyncAction::Upload | SyncAction::Download);
        let right_file = matches!(right_action, SyncAction::Upload | SyncAction::Download);
        left_file
            .cmp(&right_file)
            .then_with(|| left_path.matches('/').count().cmp(&right_path.matches('/').count()))
            .then_with(|| left_path.cmp(right_path))
    });

    let total_items = items.len();
    let control = Arc::new(TransferControl::new());
    state.transfer_controls.lock().await.insert(request.sync_id.clone(), control.clone());
    let file_system = connection.file_system();
    let mut completed_items = 0;
    let mut transferred_bytes = 0;
    let mut log = Vec::new();
    let mut final_status = "Completed".to_string();

    for (relative, action) in items {
        if let Err(error) = control.wait_until_running().await {
            final_status = "Cancelled".to_string();
            log.push(SyncExecutionLogItem {
                path: relative,
                action,
                status: final_status.clone(),
                detail: error,
                bytes: 0,
            });
            break;
        }
        let relative_path = safe_relative_path(&relative)?;
        let local_path = canonical_local_root.join(&relative_path);
        let remote_path = remote_join(&request.remote_directory, &relative_path);
        let _ = app.emit(
            "sync://progress",
            SyncExecutionProgress {
                sync_id: request.sync_id.clone(),
                completed_items,
                total_items,
                current_path: relative.clone(),
                status: "Running".to_string(),
            },
        );
        let operation = match action {
            SyncAction::CreateRemoteDirectory => {
                file_system.create_dir(&remote_path).await.map(|_| 0).map_err(|error| error.to_string())
            }
            SyncAction::CreateLocalDirectory => reject_symlink_ancestors(&canonical_local_root, &local_path)
                .and_then(|_| std::fs::create_dir_all(&local_path).map_err(|error| error.to_string()))
                .map(|_| 0),
            SyncAction::Upload => {
                let canonical_file = local_path.canonicalize().map_err(|error| error.to_string());
                match canonical_file {
                    Ok(path) if path.starts_with(&canonical_local_root) && path.is_file() => file_system
                        .upload_file(&path.to_string_lossy(), &remote_path)
                        .await
                        .map_err(|error| error.to_string()),
                    Ok(_) => Err(format!("Local sync source escaped its root: {relative}")),
                    Err(error) => Err(error),
                }
            }
            SyncAction::Download => {
                let validation =
                    reject_symlink_ancestors(&canonical_local_root, &local_path).and_then(|_| {
                        local_path.parent().ok_or("Invalid local destination.".to_string()).and_then(
                            |parent| std::fs::create_dir_all(parent).map_err(|error| error.to_string()),
                        )
                    });
                match validation {
                    Ok(()) => file_system
                        .download_file(&remote_path, &local_path.to_string_lossy())
                        .await
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error),
                }
            }
            SyncAction::Conflict | SyncAction::DestinationOnly => {
                Err("Unsafe sync action was rejected.".to_string())
            }
        };
        match operation {
            Ok(bytes) => {
                completed_items += 1;
                transferred_bytes += bytes;
                log.push(SyncExecutionLogItem {
                    path: relative.clone(),
                    action,
                    status: "Completed".to_string(),
                    detail: String::new(),
                    bytes,
                });
                let _ = app.emit(
                    "sync://progress",
                    SyncExecutionProgress {
                        sync_id: request.sync_id.clone(),
                        completed_items,
                        total_items,
                        current_path: relative,
                        status: "Running".to_string(),
                    },
                );
            }
            Err(error) => {
                final_status = if error.to_lowercase().contains("cancel") {
                    "Cancelled".to_string()
                } else {
                    "Failed".to_string()
                };
                log.push(SyncExecutionLogItem {
                    path: relative,
                    action,
                    status: final_status.clone(),
                    detail: error,
                    bytes: 0,
                });
                break;
            }
        }
    }

    drop(connections);
    state.transfer_controls.lock().await.remove(&request.sync_id);
    let result = SyncExecutionResult {
        sync_id: request.sync_id.clone(),
        status: final_status.clone(),
        completed_items,
        total_items,
        bytes: transferred_bytes,
        log,
    };
    let detail = serde_json::to_string(&result.log).map_err(|error| error.to_string())?;
    state.bookmarks.record_sync_history(&SyncHistory {
        id: request.sync_id.clone(),
        direction: direction_label(request.direction).to_string(),
        local_directory: request.local_directory,
        remote_directory: request.remote_directory,
        status: final_status.clone(),
        completed_items: completed_items as u64,
        total_items: total_items as u64,
        bytes: transferred_bytes,
        detail,
        completed_at: String::new(),
    })?;
    let _ = app.emit(
        "sync://progress",
        SyncExecutionProgress {
            sync_id: request.sync_id,
            completed_items,
            total_items,
            current_path: String::new(),
            status: final_status,
        },
    );
    Ok(result)
}

#[tauri::command]
pub async fn remote_create_directory(
    request: RemotePathRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let mut connections = state.connections.lock().await;
    connections
        .get_mut(&request.connection_id)
        .ok_or("Connection not found.")?
        .file_system()
        .create_dir(&request.path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remote_rename(request: RenameRequest, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut connections = state.connections.lock().await;
    connections
        .get_mut(&request.connection_id)
        .ok_or("Connection not found.")?
        .file_system()
        .rename(&request.old_path, &request.new_path)
        .await
        .map_err(|error| error.to_string())
}

fn remote_parent_and_name(path: &str) -> Result<(String, String), String> {
    let normalized = path.trim_end_matches('/');
    if !normalized.starts_with('/') || normalized.is_empty() {
        return Err("A remote path must be absolute.".to_string());
    }
    let separator = normalized.rfind('/').ok_or("Invalid remote path.")?;
    let name = &normalized[separator + 1..];
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err("Invalid remote item name.".to_string());
    }
    let parent = if separator == 0 { "/" } else { &normalized[..separator] };
    Ok((parent.to_string(), name.to_string()))
}

fn remote_child(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{}/{name}", parent.trim_end_matches('/'))
    }
}

#[tauri::command]
pub async fn remote_paste(
    request: RemotePasteRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if request.source_path == "/" || request.destination_path == "/" {
        return Err("The remote root cannot be copied or moved.".to_string());
    }
    let (destination_parent, destination_name) = remote_parent_and_name(&request.destination_path)?;
    remote_parent_and_name(&request.source_path)?;
    if request.source_path == request.destination_path {
        return Err("The source and destination are the same.".to_string());
    }
    if request.is_directory
        && request.destination_path.starts_with(&format!("{}/", request.source_path.trim_end_matches('/')))
    {
        return Err("A directory cannot be pasted inside itself.".to_string());
    }

    let mut connections = state.connections.lock().await;
    let file_system =
        connections.get_mut(&request.connection_id).ok_or("Connection not found.")?.file_system();
    let destination_entries =
        file_system.list_dir(&destination_parent).await.map_err(|error| error.to_string())?;
    if destination_entries.iter().any(|entry| entry.name == destination_name) {
        return Err(format!("An item named '{destination_name}' already exists at the destination."));
    }

    if request.cut {
        return file_system
            .rename(&request.source_path, &request.destination_path)
            .await
            .map_err(|error| error.to_string());
    }

    let copy_root = state
        .drag_cache_directory
        .join(format!("remote-copy-{}", REMOTE_COPY_SEQUENCE.fetch_add(1, Ordering::Relaxed)));
    tokio::fs::create_dir_all(&copy_root).await.map_err(|error| error.to_string())?;
    let result = async {
        let mut pending =
            vec![(request.source_path.clone(), request.destination_path.clone(), request.is_directory)];
        let mut visited = 0usize;
        while let Some((source, destination, is_directory)) = pending.pop() {
            visited += 1;
            if visited > 100_000 {
                return Err("Remote copy stopped after 100,000 items.".to_string());
            }
            if is_directory {
                file_system.create_dir(&destination).await.map_err(|error| error.to_string())?;
                let children = file_system.list_dir(&source).await.map_err(|error| error.to_string())?;
                for child in children.into_iter().rev() {
                    if matches!(child.file_type, crate::sftp_client::FileEntryType::Symlink) {
                        return Err(format!(
                            "Copying symbolic links is not supported: {}",
                            remote_child(&source, &child.name)
                        ));
                    }
                    pending.push((
                        remote_child(&source, &child.name),
                        remote_child(&destination, &child.name),
                        matches!(child.file_type, crate::sftp_client::FileEntryType::Directory),
                    ));
                }
            } else {
                let temporary = copy_root.join(format!("item-{visited}.tmp"));
                file_system
                    .download_file(&source, &temporary.to_string_lossy())
                    .await
                    .map_err(|error| error.to_string())?;
                let upload = file_system
                    .upload_file(&temporary.to_string_lossy(), &destination)
                    .await
                    .map_err(|error| error.to_string());
                let _ = tokio::fs::remove_file(&temporary).await;
                upload?;
            }
        }
        Ok(())
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&copy_root).await;
    result
}

#[tauri::command]
pub async fn remote_set_metadata(
    request: SetMetadataRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if request.permissions.is_none()
        && request.modified.is_none()
        && request.owner_id.is_none()
        && request.group_id.is_none()
    {
        return Err("Choose at least one file-information field to change.".to_string());
    }
    if request.permissions.is_some_and(|mode| mode > 0o7777) {
        return Err("Permissions must be an octal value between 0000 and 7777.".to_string());
    }
    let modified = request
        .modified
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| "The modification date is outside the supported range.".to_string())
        })
        .transpose()?;
    let mut connections = state.connections.lock().await;
    connections
        .get_mut(&request.connection_id)
        .ok_or("Connection not found.")?
        .file_system()
        .set_metadata(&request.path, request.permissions, modified, request.owner_id, request.group_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remote_delete(request: DeleteRequest, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut connections = state.connections.lock().await;
    let file_system =
        connections.get_mut(&request.connection_id).ok_or("Connection not found.")?.file_system();
    if request.is_directory {
        file_system.delete_dir(&request.path).await
    } else {
        file_system.delete_file(&request.path).await
    }
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remote_delete_tree(
    request: DeleteRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let target = remote_replace_target(&request.path)?;

    let mut connections = state.connections.lock().await;
    let file_system =
        connections.get_mut(&request.connection_id).ok_or("Connection not found.")?.file_system();

    if !request.is_directory {
        return file_system.delete_file(&target).await.map_err(|error| error.to_string());
    }

    // Delete children before their parents so this works for protocols whose
    // remove-directory operation only accepts empty directories (SFTP/FTP).
    let mut pending = vec![(target.to_string(), false)];
    while let Some((directory, children_visited)) = pending.pop() {
        if children_visited {
            file_system.delete_dir(&directory).await.map_err(|error| error.to_string())?;
            continue;
        }

        let children = file_system.list_dir(&directory).await.map_err(|error| error.to_string())?;
        pending.push((directory.clone(), true));
        for child in children.into_iter().rev() {
            let child_path = remote_child_path(&directory, &child.name)?;
            if child.file_type == crate::sftp_client::FileEntryType::Directory {
                pending.push((child_path, false));
            } else {
                file_system.delete_file(&child_path).await.map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn transfer_upload(
    request: TransferRequest,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<u64, String> {
    let transfer_id = request.transfer_id.unwrap_or_else(|| "single-upload".to_string());
    let started = std::time::Instant::now();
    let control = Arc::new(TransferControl::new());
    state.transfer_controls.lock().await.insert(transfer_id.clone(), control.clone());
    let mut connections = state.connections.lock().await;
    let result = match connections.get_mut(&request.connection_id).ok_or("Connection not found.")? {
        RemoteConnection::Sftp(client) => {
            let event_app = app.clone();
            let event_id = transfer_id.clone();
            client
                .upload_file_with_progress(&request.local_path, &request.remote_path, move |done, total| {
                    let event_app = event_app.clone();
                    let event_id = event_id.clone();
                    let control = control.clone();
                    async move {
                        control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                        let _ = event_app.emit(
                            "transfer://file-progress",
                            FileTransferProgress {
                                transfer_id: event_id,
                                transferred_bytes: done,
                                total_bytes: total,
                                elapsed_ms: started.elapsed().as_millis() as u64,
                                status: "running".to_string(),
                            },
                        );
                        Ok(())
                    }
                })
                .await
        }
        RemoteConnection::Ftp { client, .. } => {
            client.upload_file(&request.local_path, &request.remote_path).await
        }
        RemoteConnection::WebDav(client) => {
            client.upload_file(&request.local_path, &request.remote_path).await
        }
        RemoteConnection::S3(client) => {
            let event_app = app.clone();
            let event_id = transfer_id.clone();
            client
                .upload_file_with_progress(&request.local_path, &request.remote_path, move |done, total| {
                    let event_app = event_app.clone();
                    let event_id = event_id.clone();
                    let control = control.clone();
                    async move {
                        control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                        let _ = event_app.emit(
                            "transfer://file-progress",
                            FileTransferProgress {
                                transfer_id: event_id,
                                transferred_bytes: done,
                                total_bytes: total,
                                elapsed_ms: started.elapsed().as_millis() as u64,
                                status: "running".to_string(),
                            },
                        );
                        Ok(())
                    }
                })
                .await
        }
    };
    drop(connections);
    state.transfer_controls.lock().await.remove(&transfer_id);
    let bytes = result.map_err(|error| error.to_string())?;
    let _ = app.emit(
        "transfer://file-progress",
        FileTransferProgress {
            transfer_id,
            transferred_bytes: bytes,
            total_bytes: bytes,
            elapsed_ms: started.elapsed().as_millis() as u64,
            status: "completed".to_string(),
        },
    );
    Ok(bytes)
}

#[tauri::command]
pub async fn transfer_download(
    request: TransferRequest,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<u64, String> {
    let transfer_id = request.transfer_id.unwrap_or_else(|| "single-download".to_string());
    let started = std::time::Instant::now();
    let control = Arc::new(TransferControl::new());
    state.transfer_controls.lock().await.insert(transfer_id.clone(), control.clone());
    let mut connections = state.connections.lock().await;
    let result = match connections.get_mut(&request.connection_id).ok_or("Connection not found.")? {
        RemoteConnection::Sftp(client) => {
            let event_app = app.clone();
            let event_id = transfer_id.clone();
            client
                .download_file_with_progress(&request.remote_path, &request.local_path, move |done, total| {
                    let event_app = event_app.clone();
                    let event_id = event_id.clone();
                    let control = control.clone();
                    async move {
                        control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                        let _ = event_app.emit(
                            "transfer://file-progress",
                            FileTransferProgress {
                                transfer_id: event_id,
                                transferred_bytes: done,
                                total_bytes: total,
                                elapsed_ms: started.elapsed().as_millis() as u64,
                                status: "running".to_string(),
                            },
                        );
                        Ok(())
                    }
                })
                .await
        }
        RemoteConnection::Ftp { client, .. } => {
            client.download_file(&request.remote_path, &request.local_path).await
        }
        RemoteConnection::WebDav(client) => {
            client.download_file(&request.remote_path, &request.local_path).await
        }
        RemoteConnection::S3(client) => {
            let event_app = app.clone();
            let event_id = transfer_id.clone();
            client
                .download_file_with_progress(&request.remote_path, &request.local_path, move |done, total| {
                    let event_app = event_app.clone();
                    let event_id = event_id.clone();
                    let control = control.clone();
                    async move {
                        control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                        let _ = event_app.emit(
                            "transfer://file-progress",
                            FileTransferProgress {
                                transfer_id: event_id,
                                transferred_bytes: done,
                                total_bytes: total,
                                elapsed_ms: started.elapsed().as_millis() as u64,
                                status: "running".to_string(),
                            },
                        );
                        Ok(())
                    }
                })
                .await
        }
    };
    drop(connections);
    state.transfer_controls.lock().await.remove(&transfer_id);
    let bytes = result.map_err(|error| error.to_string())?;
    let _ = app.emit(
        "transfer://file-progress",
        FileTransferProgress {
            transfer_id,
            transferred_bytes: bytes,
            total_bytes: bytes,
            elapsed_ms: started.elapsed().as_millis() as u64,
            status: "completed".to_string(),
        },
    );
    Ok(bytes)
}

#[tauri::command]
pub async fn transfer_upload_directory(
    request: DirectoryTransferRequest,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let root = PathBuf::from(&request.local_directory);
    if !root.is_dir() {
        return Err("The selected local path is not a directory.".to_string());
    }
    let mut entries = Vec::new();
    collect_local_entries(&root, &root, &mut entries)?;
    let total_files = entries.iter().filter(|entry| matches!(entry, LocalEntry::File(_, _))).count();
    let mut completed_files = 0;
    let control = Arc::new(TransferControl::new());
    state.transfer_controls.lock().await.insert(request.transfer_id.clone(), control.clone());
    let mut connections = state.connections.lock().await;
    let connection = connections.get_mut(&request.connection_id).ok_or("Connection not found.")?;

    // The selected folder itself must exist before empty folders or nested
    // files can be transferred. An existing destination is harmless.
    let _ = connection.file_system().create_dir(&request.remote_directory).await;

    for entry in entries {
        if let Err(error) = control.wait_until_running().await {
            state.transfer_controls.lock().await.remove(&request.transfer_id);
            return Err(error);
        }
        match entry {
            LocalEntry::Directory(relative_path) => {
                let remote_path = remote_join(&request.remote_directory, &relative_path);
                let _ = connection.file_system().create_dir(&remote_path).await;
            }
            LocalEntry::File(local_path, relative_path) => {
                let remote_path = remote_join(&request.remote_directory, &relative_path);
                let local_path_string = local_path.to_string_lossy().to_string();
                let upload_result = match connection {
                    RemoteConnection::S3(client) => {
                        let file_control = control.clone();
                        client
                            .upload_file_with_progress(&local_path_string, &remote_path, move |_, _| {
                                let file_control = file_control.clone();
                                async move { file_control.wait_until_running().await.map_err(anyhow::Error::msg) }
                            })
                            .await
                    }
                    _ => connection.file_system().upload_file(&local_path_string, &remote_path).await,
                };
                if let Err(error) = upload_result {
                    state.transfer_controls.lock().await.remove(&request.transfer_id);
                    return Err(error.to_string());
                }
                completed_files += 1;
                let _ = app.emit(
                    "transfer://progress",
                    DirectoryTransferProgress {
                        transfer_id: request.transfer_id.clone(),
                        completed_files,
                        total_files,
                        current_path: relative_path.to_string_lossy().to_string(),
                        status: "running".to_string(),
                    },
                );
            }
        }
    }
    let _ = app.emit(
        "transfer://progress",
        DirectoryTransferProgress {
            transfer_id: request.transfer_id.clone(),
            completed_files,
            total_files,
            current_path: request.remote_directory,
            status: "completed".to_string(),
        },
    );
    state.transfer_controls.lock().await.remove(&request.transfer_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        content_hash, parse_remote_modified, reject_symlink_ancestors, remote_child_path,
        remote_parent_and_name, remote_replace_target, safe_relative_path, TransferControl,
    };
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    #[test]
    fn editing_cache_hash_changes_with_file_contents() {
        let root = tempfile::tempdir().expect("cache root");
        let file = root.path().join("file.txt");
        std::fs::write(&file, b"before").expect("initial cache");
        let before = content_hash(&file).expect("initial hash");
        std::fs::write(&file, b"after").expect("edited cache");
        assert_ne!(before, content_hash(&file).expect("edited hash"));
    }

    #[tokio::test]
    async fn cancelling_a_paused_transfer_unblocks_the_waiter() {
        let control = Arc::new(TransferControl::new());
        control.paused.store(true, Ordering::Release);
        let waiter_control = control.clone();
        let waiter = tokio::spawn(async move { waiter_control.wait_until_running().await });
        tokio::task::yield_now().await;
        control.cancelled.store(true, Ordering::Release);
        control.resumed.notify_waiters();
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("cancelled wait should finish")
            .expect("wait task should not panic");
        assert_eq!(result.unwrap_err(), "Transfer cancelled.");
    }

    #[test]
    fn parses_protocol_modified_times_for_rsync_quick_checks() {
        let expected = 1_777_111_200;
        assert_eq!(parse_remote_modified("2026-04-25 10:00:00"), Some(expected));
        assert_eq!(parse_remote_modified("Sat, 25 Apr 2026 10:00:00 GMT"), Some(expected));
        assert_eq!(parse_remote_modified(&expected.to_string()), Some(expected));
        assert_eq!(parse_remote_modified("unknown"), None);
    }

    #[test]
    fn rejects_absolute_and_parent_sync_paths() {
        assert!(safe_relative_path("folder/file.txt").is_ok());
        assert!(safe_relative_path("/tmp/file.txt").is_err());
        assert!(safe_relative_path("../file.txt").is_err());
        assert!(safe_relative_path("folder/../file.txt").is_err());
    }

    #[test]
    fn validates_destructive_remote_replace_paths() {
        assert_eq!(remote_replace_target("/uploads/site/").unwrap(), "/uploads/site");
        assert!(remote_replace_target("/").is_err());
        assert!(remote_replace_target("relative/folder").is_err());
        assert!(remote_replace_target("/uploads/../private").is_err());
        assert_eq!(remote_child_path("/uploads", "港便り").unwrap(), "/uploads/港便り");
        assert!(remote_child_path("/uploads", "../private").is_err());
        assert!(remote_child_path("/uploads", "nested/file.txt").is_err());
    }

    #[test]
    fn splits_remote_paste_destinations_safely() {
        assert_eq!(
            remote_parent_and_name("/documents/港便り.txt").unwrap(),
            ("/documents".to_string(), "港便り.txt".to_string())
        );
        assert_eq!(
            remote_parent_and_name("/港便り.txt").unwrap(),
            ("/".to_string(), "港便り.txt".to_string())
        );
        assert!(remote_parent_and_name("relative.txt").is_err());
        assert!(remote_parent_and_name("/").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_local_sync_destinations_through_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("local root");
        let outside = tempfile::tempdir().expect("outside directory");
        symlink(outside.path(), root.path().join("escape")).expect("create symlink");
        assert!(reject_symlink_ancestors(root.path(), &root.path().join("escape/file.txt")).is_err());
        assert!(reject_symlink_ancestors(root.path(), &root.path().join("safe/file.txt")).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn editing_cache_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("cache root");
        let source = root.path().join("source.txt");
        let link = root.path().join("link.txt");
        std::fs::write(&source, b"secret").expect("source file");
        symlink(&source, &link).expect("cache symlink");
        assert!(content_hash(&link).is_err());
    }
}

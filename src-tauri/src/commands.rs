use crate::bookmarks::{Bookmark, BookmarkStore, ConnectionHistory, TransferHistory};
use crate::ftp_client::{FtpClient, FtpConfig};
use crate::remote_fs::RemoteFileSystem;
use crate::sftp_client::{FileEntry, SftpAuthMethod, SftpConfig, StandaloneSftpClient};
use crate::ssh;
use crate::sync::{plan_sync, SnapshotEntry, SyncDirection, SyncPreview};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::Mutex;

pub struct AppState {
    connections: Mutex<HashMap<String, RemoteConnection>>,
    bookmarks: BookmarkStore,
    transfer_controls: Mutex<HashMap<String, Arc<TransferControl>>>,
}

impl AppState {
    pub fn new(data_directory: std::path::PathBuf) -> Result<Self, String> {
        Ok(Self {
            connections: Mutex::new(HashMap::new()),
            bookmarks: BookmarkStore::new(&data_directory)?,
            transfer_controls: Mutex::new(HashMap::new()),
        })
    }
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
        while self.paused.load(Ordering::Acquire) {
            self.resumed.notified().await;
        }
        if self.cancelled.load(Ordering::Acquire) {
            Err("Transfer cancelled.".to_string())
        } else {
            Ok(())
        }
    }
}

enum RemoteConnection {
    Sftp(StandaloneSftpClient),
    Ftp { client: FtpClient, protocol: Protocol },
}

impl RemoteConnection {
    fn file_system(&mut self) -> &mut dyn RemoteFileSystem {
        match self {
            Self::Sftp(client) => client,
            Self::Ftp { client, .. } => client,
        }
    }

    fn protocol(&self) -> Protocol {
        match self {
            Self::Sftp(_) => Protocol::Sftp,
            Self::Ftp { protocol, .. } => *protocol,
        }
    }
}

#[derive(Debug, Deserialize)]
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
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Sftp,
    Ftp,
    Ftps,
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
            let size =
                if is_directory { 0 } else { entry.metadata().map_err(|error| error.to_string())?.len() };
            snapshot.push(SnapshotEntry { path: relative, size, is_directory });
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
            snapshot.push(SnapshotEntry { path: relative.clone(), size: entry.size, is_directory });
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

/// Metadata only: private-key bytes never cross the Tauri IPC boundary.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeySummary {
    pub name: String,
    pub path: String,
    pub public_key_path: Option<String>,
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
                || name.ends_with(".pub")
                || ignored_names.contains(&name.as_str())
            {
                return None;
            }
            let public_key_path = path.with_file_name(format!("{name}.pub"));
            Some(SshKeySummary {
                kind: if name.starts_with("id_") { "OpenSSH".to_string() } else { "SSH key".to_string() },
                name,
                path: path.to_string_lossy().to_string(),
                public_key_path: public_key_path
                    .is_file()
                    .then(|| public_key_path.to_string_lossy().to_string()),
            })
        })
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
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
    let local = collect_local_snapshot(Path::new(&request.local_directory))?;
    let mut connections = state.connections.lock().await;
    let connection = connections.get_mut(&request.connection_id).ok_or("Connection not found.")?;
    let remote = collect_remote_snapshot(connection.file_system(), &request.remote_directory).await?;
    Ok(plan_sync(local, remote, request.direction))
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
    let file_system = connection.file_system();

    // The selected folder itself must exist before empty folders or nested
    // files can be transferred. An existing destination is harmless.
    let _ = file_system.create_dir(&request.remote_directory).await;

    for entry in entries {
        if let Err(error) = control.wait_until_running().await {
            state.transfer_controls.lock().await.remove(&request.transfer_id);
            return Err(error);
        }
        match entry {
            LocalEntry::Directory(relative_path) => {
                let remote_path = remote_join(&request.remote_directory, &relative_path);
                let _ = file_system.create_dir(&remote_path).await;
            }
            LocalEntry::File(local_path, relative_path) => {
                let remote_path = remote_join(&request.remote_directory, &relative_path);
                let local_path_string = local_path.to_string_lossy().to_string();
                file_system
                    .upload_file(&local_path_string, &remote_path)
                    .await
                    .map_err(|error| error.to_string())?;
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

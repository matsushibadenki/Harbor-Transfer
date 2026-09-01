use crate::bookmarks::{
    Bookmark, BookmarkStore, ConnectionHistory, SyncHistory, TransferHistory, TransferJob, TransferLogEvent,
};
use crate::ftp_client::{FtpClient, FtpConfig};
use crate::google_drive::{
    self, GoogleAuthorizationStatus, GoogleDriveClient, GoogleDriveUploadState, GoogleExportOptions,
};
use crate::remote_fs::RemoteFileSystem;
use crate::s3_client::{S3Client, S3Config, S3MultipartState};
use crate::samba_client::{SambaClient, SambaConfig};
use crate::secret_store::{self, SecretLookup};
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
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, State};
use tokio::sync::Mutex;

static EDIT_SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static DRAG_EXPORT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static REMOTE_COPY_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const MAX_AUTOMATIC_TRANSFER_RETRIES: u32 = 3;
const DEFAULT_MAX_CONCURRENT_TRANSFERS: u32 = 3;

#[derive(Default)]
struct TransferAdmissionState {
    active_total: u32,
    active_by_connection: HashMap<String, u32>,
}

struct TransferScheduler {
    state: StdMutex<TransferAdmissionState>,
    changed: tokio::sync::Notify,
}

impl TransferScheduler {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: StdMutex::new(TransferAdmissionState::default()),
            changed: tokio::sync::Notify::new(),
        })
    }

    #[cfg(test)]
    async fn acquire(
        self: &Arc<Self>,
        connection_id: &str,
        global_limit: u32,
        connection_limit: u32,
    ) -> TransferPermit {
        let global_limit = global_limit.clamp(1, 16);
        let connection_limit = connection_limit.clamp(1, 16).min(global_limit);
        loop {
            let changed = self.changed.notified();
            {
                let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let connection_active = state.active_by_connection.get(connection_id).copied().unwrap_or(0);
                if state.active_total < global_limit && connection_active < connection_limit {
                    state.active_total += 1;
                    *state.active_by_connection.entry(connection_id.to_string()).or_default() += 1;
                    return TransferPermit {
                        scheduler: self.clone(),
                        connection_id: connection_id.to_string(),
                    };
                }
            }
            changed.await;
        }
    }

    async fn acquire_cancellable(
        self: &Arc<Self>,
        connection_id: &str,
        global_limit: u32,
        connection_limit: u32,
        control: &TransferControl,
    ) -> Result<TransferPermit, String> {
        let global_limit = global_limit.clamp(1, 16);
        let connection_limit = connection_limit.clamp(1, 16).min(global_limit);
        loop {
            if control.cancelled.load(Ordering::Acquire) {
                return Err("Transfer cancelled.".to_string());
            }
            let changed = self.changed.notified();
            if !control.paused.load(Ordering::Acquire) {
                let mut state = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let connection_active = state.active_by_connection.get(connection_id).copied().unwrap_or(0);
                if state.active_total < global_limit && connection_active < connection_limit {
                    state.active_total += 1;
                    *state.active_by_connection.entry(connection_id.to_string()).or_default() += 1;
                    return Ok(TransferPermit {
                        scheduler: self.clone(),
                        connection_id: connection_id.to_string(),
                    });
                }
            }
            changed.await;
        }
    }
}

struct TransferPermit {
    scheduler: Arc<TransferScheduler>,
    connection_id: String,
}

impl Drop for TransferPermit {
    fn drop(&mut self) {
        let mut state = self.scheduler.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.active_total = state.active_total.saturating_sub(1);
        if let Some(active) = state.active_by_connection.get_mut(&self.connection_id) {
            *active = active.saturating_sub(1);
            if *active == 0 {
                state.active_by_connection.remove(&self.connection_id);
            }
        }
        drop(state);
        self.scheduler.changed.notify_waiters();
    }
}

#[derive(Default)]
struct BandwidthSchedule {
    global_next: Option<Instant>,
    connection_next: HashMap<String, Instant>,
}

struct BandwidthLimiter {
    schedule: StdMutex<BandwidthSchedule>,
}

impl BandwidthLimiter {
    fn new() -> Arc<Self> {
        Arc::new(Self { schedule: StdMutex::new(BandwidthSchedule::default()) })
    }

    async fn throttle(&self, connection_id: &str, bytes: u64, global_bps: u64, connection_bps: u64) {
        if bytes == 0 || (global_bps == 0 && connection_bps == 0) {
            return;
        }
        let deadline = {
            let now = Instant::now();
            let mut schedule = self.schedule.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut deadline = now;
            if global_bps > 0 {
                deadline = reserve_bandwidth(&mut schedule.global_next, now, bytes, global_bps);
            }
            if connection_bps > 0 {
                let next = schedule.connection_next.entry(connection_id.to_string()).or_insert(now);
                let connection_deadline = reserve_bandwidth_instant(next, now, bytes, connection_bps);
                deadline = deadline.max(connection_deadline);
            }
            deadline
        };
        if deadline > Instant::now() {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        }
    }
}

fn reserve_bandwidth(slot: &mut Option<Instant>, now: Instant, bytes: u64, bytes_per_second: u64) -> Instant {
    let next = slot.get_or_insert(now);
    reserve_bandwidth_instant(next, now, bytes, bytes_per_second)
}

fn reserve_bandwidth_instant(next: &mut Instant, now: Instant, bytes: u64, bytes_per_second: u64) -> Instant {
    let start = (*next).max(now);
    let seconds = bytes as f64 / bytes_per_second.max(1) as f64;
    let deadline = start + Duration::from_secs_f64(seconds.min(86_400.0));
    *next = deadline;
    deadline
}

fn retry_backoff(attempt: u32) -> Duration {
    Duration::from_millis(500u64.saturating_mul(1u64 << attempt.min(3)))
}

fn is_retryable_transfer_error(error: &anyhow::Error) -> bool {
    let messages = error.chain().map(|cause| cause.to_string().to_ascii_lowercase()).collect::<Vec<_>>();
    let permanent = [
        "authentication",
        "unauthorized",
        "forbidden",
        "bad credentials",
        "permission denied",
        "connection not found",
        "not found",
        "no such file",
        "invalid",
        "unsafe",
        "checksum verification failed",
        "size verification failed",
        "transfer cancelled",
        "already exists",
        "quota",
    ];
    if permanent.iter().any(|needle| messages.iter().any(|message| message.contains(needle))) {
        return false;
    }
    [
        "timed out",
        "timeout",
        "connection",
        "broken pipe",
        "unexpected eof",
        "temporarily unavailable",
        "too many requests",
        "rate limit",
        "http 429",
        "http 500",
        "http 502",
        "http 503",
        "http 504",
        "connection reset",
    ]
    .iter()
    .any(|needle| messages.iter().any(|message| message.contains(needle)))
}

pub struct AppState {
    connections: Mutex<HashMap<String, Arc<Mutex<RemoteConnection>>>>,
    credential_cache: Mutex<HashMap<String, Option<String>>>,
    bookmarks: BookmarkStore,
    transfer_controls: Mutex<HashMap<String, Arc<TransferControl>>>,
    transfer_scheduler: Arc<TransferScheduler>,
    bandwidth_limiter: Arc<BandwidthLimiter>,
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
            credential_cache: Mutex::new(HashMap::new()),
            bookmarks: BookmarkStore::new(&data_directory)?,
            transfer_controls: Mutex::new(HashMap::new()),
            transfer_scheduler: TransferScheduler::new(),
            bandwidth_limiter: BandwidthLimiter::new(),
            edit_cache_directory,
            remote_edits: Mutex::new(HashMap::new()),
            drag_cache_directory,
            drag_icon_path,
            drag_exports: Mutex::new(HashMap::new()),
        })
    }

    async fn connection(&self, connection_id: &str) -> Option<Arc<Mutex<RemoteConnection>>> {
        self.connections.lock().await.get(connection_id).cloned()
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
    Sftp { client: StandaloneSftpClient, protocol: Protocol },
    Ftp { client: FtpClient, protocol: Protocol },
    WebDav(WebDavClient),
    S3(S3Client),
    Samba(Box<SambaClient>),
    GoogleDrive(GoogleDriveClient),
}

impl RemoteConnection {
    fn file_system(&mut self) -> &mut dyn RemoteFileSystem {
        match self {
            Self::Sftp { client, .. } => client,
            Self::Ftp { client, .. } => client,
            Self::WebDav(client) => client,
            Self::S3(client) => client,
            Self::Samba(client) => client.as_mut(),
            Self::GoogleDrive(client) => client,
        }
    }

    fn protocol(&self) -> Protocol {
        match self {
            Self::Sftp { protocol, .. } => *protocol,
            Self::Ftp { protocol, .. } => *protocol,
            Self::WebDav(_) => Protocol::Webdav,
            Self::S3(_) => Protocol::S3,
            Self::Samba(_) => Protocol::Smb,
            Self::GoogleDrive(_) => Protocol::GoogleDrive,
        }
    }

    async fn reconnect(&mut self) -> anyhow::Result<()> {
        match self {
            Self::Sftp { client, .. } => client.reconnect().await,
            Self::Ftp { client, .. } => client.reconnect().await,
            Self::WebDav(client) => client.reconnect().await,
            // AWS SDK, Google HTTP requests, and smb2 already have their own
            // connection pools/retry or auto-reconnect behavior.
            Self::S3(_) | Self::GoogleDrive(_) => Ok(()),
            Self::Samba(client) => client.reconnect().await,
        }
    }

    async fn duplicate_for_transfer(&self) -> anyhow::Result<Self> {
        match self {
            Self::Sftp { client, protocol } => {
                Ok(Self::Sftp { client: client.duplicate().await?, protocol: *protocol })
            }
            Self::Ftp { client, protocol } => {
                Ok(Self::Ftp { client: client.duplicate().await?, protocol: *protocol })
            }
            Self::WebDav(client) => Ok(Self::WebDav(client.clone())),
            Self::S3(client) => Ok(Self::S3(client.clone())),
            Self::Samba(client) => Ok(Self::Samba(Box::new(client.duplicate().await?))),
            Self::GoogleDrive(client) => Ok(Self::GoogleDrive(client.clone())),
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
    pub smb_share: Option<String>,
    pub smb_domain: Option<String>,
    #[serde(default)]
    pub smb_guest: bool,
    pub google_client_id: Option<String>,
    pub google_drive_location_kind: Option<String>,
    pub google_drive_location_id: Option<String>,
    pub google_docs_export: Option<String>,
    pub google_sheets_export: Option<String>,
    pub google_slides_export: Option<String>,
    pub google_drawings_export: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Sftp,
    #[serde(rename = "cloudFtp")]
    CloudFtp,
    Ftp,
    Ftps,
    Webdav,
    S3,
    Smb,
    #[serde(rename = "googleDrive")]
    GoogleDrive,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSummary {
    pub connection_id: String,
    pub protocol: Protocol,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferOutcome {
    pub bytes: u64,
    /// `sha256` means an independently re-read remote object matched the
    /// local digest. `size` is the explicit fallback for protocols that do not
    /// expose a portable server-side checksum.
    pub verification: String,
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
    pub name: Option<String>,
    pub conflict_policy: Option<String>,
    pub resume_from: Option<u64>,
    #[serde(default = "default_max_concurrent_transfers")]
    pub global_max_concurrent_transfers: u32,
    pub connection_max_concurrent_transfers: Option<u32>,
    #[serde(default)]
    pub global_bandwidth_limit_bps: u64,
    pub connection_bandwidth_limit_bps: Option<u64>,
    pub automatic_retry_count: Option<u32>,
}

fn default_max_concurrent_transfers() -> u32 {
    DEFAULT_MAX_CONCURRENT_TRANSFERS
}

#[derive(Clone, Copy)]
struct EffectiveTransferLimits {
    global_concurrency: u32,
    connection_concurrency: u32,
    global_bandwidth_bps: u64,
    connection_bandwidth_bps: u64,
    automatic_retries: u32,
}

fn effective_transfer_limits(
    global_concurrency: u32,
    connection_concurrency: Option<u32>,
    global_bandwidth_bps: u64,
    connection_bandwidth_bps: Option<u64>,
    automatic_retries: Option<u32>,
) -> EffectiveTransferLimits {
    let global_concurrency = global_concurrency.clamp(1, 16);
    EffectiveTransferLimits {
        global_concurrency,
        connection_concurrency: connection_concurrency.unwrap_or(global_concurrency).clamp(1, 16),
        global_bandwidth_bps: global_bandwidth_bps.min(10 * 1024 * 1024 * 1024),
        connection_bandwidth_bps: connection_bandwidth_bps.unwrap_or(0).min(10 * 1024 * 1024 * 1024),
        automatic_retries: automatic_retries.unwrap_or(MAX_AUTOMATIC_TRANSFER_RETRIES).min(10),
    }
}

impl TransferRequest {
    fn limits(&self) -> EffectiveTransferLimits {
        effective_transfer_limits(
            self.global_max_concurrent_transfers,
            self.connection_max_concurrent_transfers,
            self.global_bandwidth_limit_bps,
            self.connection_bandwidth_limit_bps,
            self.automatic_retry_count,
        )
    }
}

#[derive(Clone)]
struct TransferBandwidth {
    limiter: Arc<BandwidthLimiter>,
    connection_id: String,
    global_bps: u64,
    connection_bps: u64,
    last_bytes: Arc<AtomicU64>,
}

impl TransferBandwidth {
    fn new(limiter: Arc<BandwidthLimiter>, connection_id: &str, limits: EffectiveTransferLimits) -> Self {
        Self {
            limiter,
            connection_id: connection_id.to_string(),
            global_bps: limits.global_bandwidth_bps,
            connection_bps: limits.connection_bandwidth_bps,
            last_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    fn reset(&self, transferred: u64) {
        self.last_bytes.store(transferred, Ordering::Release);
    }

    async fn progress(&self, transferred: u64) {
        let previous = self.last_bytes.swap(transferred, Ordering::AcqRel);
        self.limiter
            .throttle(
                &self.connection_id,
                transferred.saturating_sub(previous),
                self.global_bps,
                self.connection_bps,
            )
            .await;
    }
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

const TRANSFER_PROGRESS_PERSIST_INTERVAL: u64 = 4 * 1024 * 1024;

fn transfer_display_name(explicit: Option<&str>, path: &str) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Path::new(path).file_name().and_then(|value| value.to_str()).map(str::to_string))
        .unwrap_or_else(|| "Transfer".to_string())
}

fn persist_progress(
    store: &BookmarkStore,
    transfer_id: &str,
    transferred: u64,
    total: u64,
    persisted: &AtomicU64,
) {
    let checkpoint =
        if transferred == total { u64::MAX } else { transferred / TRANSFER_PROGRESS_PERSIST_INTERVAL };
    let previous = persisted.load(Ordering::Relaxed);
    if checkpoint <= previous
        || persisted.compare_exchange(previous, checkpoint, Ordering::AcqRel, Ordering::Relaxed).is_err()
    {
        return;
    }
    if let Err(error) = store.update_transfer_job_progress(transfer_id, transferred, total) {
        tracing::warn!("Could not persist transfer progress: {error}");
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPathInfo {
    pub name: String,
    pub is_directory: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDirectoryEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified_unix: u64,
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDirectoryListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<LocalDirectoryEntry>,
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
    pub display_name: Option<String>,
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
    File { remote_path: String, relative: PathBuf },
}

fn safe_export_name(name: &str) -> String {
    let value = name.replace('/', "／").replace(':', "：");
    if value.is_empty() || value == "." || value == ".." {
        "Untitled".to_string()
    } else {
        value
    }
}

async fn collect_remote_export_entries(
    file_system: &mut dyn RemoteFileSystem,
    root: &str,
) -> Result<Vec<RemoteExportEntry>, String> {
    let mut result = Vec::new();
    let mut directories = vec![(root.to_string(), PathBuf::new())];
    while let Some((remote_directory, relative_directory)) = directories.pop() {
        let entries = file_system.list_dir(&remote_directory).await.map_err(|error| error.to_string())?;
        let mut local_name_counts = HashMap::<String, usize>::new();
        for entry in entries {
            let base_local_name = safe_export_name(entry.download_name.as_deref().unwrap_or(&entry.name));
            let count =
                local_name_counts.entry(base_local_name.clone()).and_modify(|value| *value += 1).or_insert(1);
            let local_name =
                if *count == 1 { base_local_name } else { format!("{base_local_name} ({count})") };
            let relative = relative_directory.join(local_name);
            let relative_text = relative.to_string_lossy();
            safe_relative_path(&relative_text)?;
            let remote_component = entry.path_component.as_deref().unwrap_or(&entry.name);
            let remote_child = remote_join(&remote_directory, Path::new(remote_component));
            match entry.file_type {
                crate::sftp_client::FileEntryType::Directory => {
                    if result.len() >= 100_000 {
                        return Err("The folder contains too many items to drag safely.".to_string());
                    }
                    result.push(RemoteExportEntry::Directory(relative.clone()));
                    directories.push((remote_child, relative));
                }
                crate::sftp_client::FileEntryType::File => {
                    if result.len() >= 100_000 {
                        return Err("The folder contains too many items to drag safely.".to_string());
                    }
                    result.push(RemoteExportEntry::File { remote_path: remote_child, relative });
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
    let name = if let Some(display_name) = request.display_name.as_deref() {
        let value = display_name.trim();
        if value.is_empty() || value == "." || value == ".." || value.contains('/') || value.contains('\\') {
            return Err("The remote file name is invalid.".to_string());
        }
        value.to_string()
    } else {
        Path::new(&request.remote_path)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or("The remote file name is invalid.")?
            .to_string()
    };
    let sequence = DRAG_EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp =
        SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_millis();
    let export_id = format!("drag-{timestamp}-{sequence}");
    let export_directory = state.drag_cache_directory.join(&export_id);
    std::fs::create_dir(&export_directory).map_err(|error| error.to_string())?;
    let local_item = export_directory.join(&name);
    let prepare = {
        let connection = state.connection(&request.connection_id).await;
        match connection {
            Some(connection) if request.is_directory => {
                let mut connection = connection.lock().await;
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
                                RemoteExportEntry::File { remote_path, relative } => {
                                    let local_file = local_item.join(&relative);
                                    let parent_result = local_file
                                        .parent()
                                        .ok_or("Invalid drag cache path.".to_string())
                                        .and_then(|parent| {
                                            std::fs::create_dir_all(parent).map_err(|error| error.to_string())
                                        });
                                    match parent_result {
                                        Ok(()) => file_system
                                            .download_file(&remote_path, &local_file.to_string_lossy())
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
                .lock()
                .await
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
        match state.connection(&request.connection_id).await {
            Some(connection) => connection
                .lock()
                .await
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
        match state.connection(&snapshot.connection_id).await {
            Some(connection) => connection
                .lock()
                .await
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

#[tauri::command]
pub async fn local_default_directory() -> Result<String, String> {
    let home = std::env::var_os("HOME").ok_or("The local home directory is unavailable.")?;
    std::fs::canonicalize(home)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn local_directory_list(path: String) -> Result<LocalDirectoryListing, String> {
    let directory =
        std::fs::canonicalize(&path).map_err(|error| format!("Could not open '{path}': {error}"))?;
    if !directory.is_dir() {
        return Err(format!("The local path is not a directory: {}", directory.display()));
    }
    let mut entries = std::fs::read_dir(&directory)
        .map_err(|error| format!("Could not read '{}': {error}", directory.display()))?
        .map(|entry| {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            let file_type = metadata.file_type();
            let kind = if file_type.is_symlink() {
                "Symlink"
            } else if metadata.is_dir() {
                "Directory"
            } else {
                "File"
            };
            let modified_unix = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |value| value.as_secs());
            Ok(LocalDirectoryEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                size: if metadata.is_file() { metadata.len() } else { 0 },
                modified_unix,
                kind: kind.to_string(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    entries.sort_by(|left, right| {
        let left_directory = left.kind == "Directory";
        let right_directory = right.kind == "Directory";
        right_directory.cmp(&left_directory).then_with(|| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.name.cmp(&right.name))
        })
    });
    let parent = directory.parent().map(|value| value.to_string_lossy().into_owned());
    Ok(LocalDirectoryListing { path: directory.to_string_lossy().into_owned(), parent, entries })
}

fn legacy_credential_entry(bookmark_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("Harbor Transfer", bookmark_id).map_err(|error| error.to_string())
}

fn credential_vault_key(bookmark_id: &str) -> String {
    format!("bookmark:{bookmark_id}")
}

#[tauri::command]
pub async fn credential_load(
    bookmark_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    let mut cache = state.credential_cache.lock().await;
    if let Some(password) = cache.get(&bookmark_id) {
        return Ok(password.clone());
    }
    let vault_key = credential_vault_key(&bookmark_id);
    let password = match secret_store::lookup(&vault_key).map_err(|error| error.to_string())? {
        SecretLookup::Value(password) => Some(password),
        SecretLookup::Removed => None,
        SecretLookup::Missing => match legacy_credential_entry(&bookmark_id)?.get_password() {
            Ok(password) => {
                if let Err(error) = secret_store::store(&vault_key, &password) {
                    tracing::warn!("Could not migrate a legacy credential into the Keychain vault: {error}");
                }
                Some(password)
            }
            Err(keyring::Error::NoEntry) => None,
            Err(error) => return Err(error.to_string()),
        },
    };
    cache.insert(bookmark_id, password.clone());
    Ok(password)
}

#[tauri::command]
pub async fn credential_save(
    bookmark_id: String,
    password: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if password.is_empty() {
        return Ok(());
    }
    let mut cache = state.credential_cache.lock().await;
    if cache.get(&bookmark_id).and_then(Option::as_deref) == Some(password.as_str()) {
        return Ok(());
    }
    secret_store::store(&credential_vault_key(&bookmark_id), &password).map_err(|error| error.to_string())?;
    cache.insert(bookmark_id, Some(password));
    Ok(())
}

#[tauri::command]
pub async fn credential_delete(bookmark_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut cache = state.credential_cache.lock().await;
    if cache.get(&bookmark_id) == Some(&None) {
        return Ok(());
    }
    secret_store::remove(&credential_vault_key(&bookmark_id)).map_err(|error| error.to_string())?;
    cache.insert(bookmark_id, None);
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryTransferRequest {
    pub transfer_id: String,
    pub connection_id: String,
    pub local_directory: String,
    pub remote_directory: String,
    pub name: Option<String>,
    pub conflict_policy: Option<String>,
    #[serde(default = "default_max_concurrent_transfers")]
    pub global_max_concurrent_transfers: u32,
    pub connection_max_concurrent_transfers: Option<u32>,
    #[serde(default)]
    pub global_bandwidth_limit_bps: u64,
    pub connection_bandwidth_limit_bps: Option<u64>,
    pub automatic_retry_count: Option<u32>,
}

impl DirectoryTransferRequest {
    fn limits(&self) -> EffectiveTransferLimits {
        effective_transfer_limits(
            self.global_max_concurrent_transfers,
            self.connection_max_concurrent_transfers,
            self.global_bandwidth_limit_bps,
            self.connection_bandwidth_limit_bps,
            self.automatic_retry_count,
        )
    }
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
    state.transfer_scheduler.changed.notify_waiters();
    state.bookmarks.set_transfer_job_status(&transfer_id, "Paused", "")?;
    Ok(())
}

#[tauri::command]
pub async fn transfer_resume(transfer_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let controls = state.transfer_controls.lock().await;
    let control = controls.get(&transfer_id).ok_or("Transfer not found.")?;
    control.paused.store(false, Ordering::Release);
    control.resumed.notify_waiters();
    state.transfer_scheduler.changed.notify_waiters();
    state.bookmarks.set_transfer_job_status(&transfer_id, "Running", "")?;
    Ok(())
}

#[tauri::command]
pub async fn transfer_cancel(transfer_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let controls = state.transfer_controls.lock().await;
    let control = controls.get(&transfer_id).ok_or("Transfer not found.")?;
    control.cancelled.store(true, Ordering::Release);
    control.resumed.notify_waiters();
    state.transfer_scheduler.changed.notify_waiters();
    let google_session_key = format!("google:upload-session:{transfer_id}");
    if matches!(secret_store::lookup(&google_session_key), Ok(SecretLookup::Value(_))) {
        secret_store::remove_ephemeral(&google_session_key).map_err(|error| error.to_string())?;
    }
    state.bookmarks.delete_transfer_job(&transfer_id)?;
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
pub async fn bookmarks_reorder(
    bookmark_ids: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.bookmarks.reorder(&bookmark_ids)
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
pub async fn transfer_jobs_list(state: State<'_, Arc<AppState>>) -> Result<Vec<TransferJob>, String> {
    state.bookmarks.transfer_jobs()
}

#[tauri::command]
pub async fn transfer_log_list(state: State<'_, Arc<AppState>>) -> Result<Vec<TransferLogEvent>, String> {
    state.bookmarks.transfer_log()
}

#[tauri::command]
pub async fn transfer_log_clear(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.bookmarks.clear_transfer_log()
}

#[tauri::command]
pub async fn transfer_job_delete(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let session_key = format!("google:upload-session:{id}");
    if matches!(secret_store::lookup(&session_key), Ok(SecretLookup::Value(_))) {
        secret_store::remove_ephemeral(&session_key).map_err(|error| error.to_string())?;
    }
    state.bookmarks.delete_transfer_job(&id)
}

#[tauri::command]
pub async fn transfer_jobs_clear(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    for job in state.bookmarks.transfer_jobs()? {
        let session_key = format!("google:upload-session:{}", job.id);
        if matches!(secret_store::lookup(&session_key), Ok(SecretLookup::Value(_))) {
            secret_store::remove_ephemeral(&session_key).map_err(|error| error.to_string())?;
        }
    }
    state.bookmarks.clear_transfer_jobs()
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
        Protocol::Sftp | Protocol::CloudFtp => {
            let auth_method = match request.key_path.filter(|value| !value.trim().is_empty()) {
                Some(key_path) => SftpAuthMethod::PublicKey { key_path, passphrase: request.passphrase },
                None if matches!(protocol, Protocol::CloudFtp) => {
                    return Err("Google Cloud FTP requires SSH public-key authentication. Select the private key paired with the public key registered for this Cloud FTP user.".to_string());
                }
                None => SftpAuthMethod::Password {
                    password: request.password.ok_or("A password or SSH key is required.")?,
                },
            };
            RemoteConnection::Sftp {
                client: StandaloneSftpClient::connect(&SftpConfig {
                    host: request.host,
                    port: request.port,
                    username: request.username,
                    auth_method,
                    expected_host_key: request.expected_host_key,
                })
                .await
                .map_err(|error| error.to_string())?,
                protocol,
            }
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
        Protocol::Smb => RemoteConnection::Samba(Box::new(
            SambaClient::connect(SambaConfig {
                host: request.host,
                port: request.port,
                share: request.smb_share.ok_or("An SMB share name is required.")?,
                username: request.username,
                password: request.password.unwrap_or_default(),
                domain: request.smb_domain.unwrap_or_default(),
                guest: request.smb_guest,
                probe_path: request.initial_path.unwrap_or_else(|| "/".to_string()),
            })
            .await
            .map_err(|error| error.to_string())?,
        )),
        Protocol::GoogleDrive => RemoteConnection::GoogleDrive(
            GoogleDriveClient::connect(
                request.google_client_id.as_deref().ok_or("A Google OAuth Client ID is required.")?,
                request.initial_path.as_deref().unwrap_or("/"),
                request.google_drive_location_kind.as_deref(),
                request.google_drive_location_id.as_deref(),
                GoogleExportOptions {
                    documents: request.google_docs_export,
                    spreadsheets: request.google_sheets_export,
                    presentations: request.google_slides_export,
                    drawings: request.google_drawings_export,
                },
            )
            .await
            .map_err(|error| error.to_string())?,
        ),
    };

    state.connections.lock().await.insert(request.connection_id.clone(), Arc::new(Mutex::new(connection)));
    Ok(ConnectionSummary { connection_id: request.connection_id, protocol })
}

#[tauri::command]
pub async fn sftp_probe_host_key(host: String, port: u16) -> Result<String, String> {
    ssh::probe_host_key(&host, port).await
}

#[tauri::command]
pub async fn google_drive_authorize(client_id: String) -> Result<GoogleAuthorizationStatus, String> {
    google_drive::authorize(client_id).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn google_drive_authorization_status(
    client_id: String,
) -> Result<GoogleAuthorizationStatus, String> {
    google_drive::authorization_status(&client_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn google_drive_locations(
    client_id: String,
) -> Result<Vec<google_drive::GoogleDriveLocation>, String> {
    GoogleDriveClient::locations(&client_id).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn google_drive_import_credentials(path: String) -> Result<String, String> {
    google_drive::import_client_credentials(&path).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn google_drive_disconnect() -> Result<(), String> {
    google_drive::disconnect_authorization().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn google_drive_open_setup_page(page: String) -> Result<(), String> {
    google_drive::open_setup_page(&page).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn connection_disconnect(
    connection_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let connection = state.connections.lock().await.remove(&connection_id);
    let connection = connection.ok_or("Connection not found.".to_string())?;
    let mut connection = connection.lock().await;
    connection.file_system().disconnect().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn connection_list(state: State<'_, Arc<AppState>>) -> Result<Vec<ConnectionSummary>, String> {
    let connections = state
        .connections
        .lock()
        .await
        .iter()
        .map(|(connection_id, connection)| (connection_id.clone(), connection.clone()))
        .collect::<Vec<_>>();
    let mut summaries = Vec::with_capacity(connections.len());
    for (connection_id, connection) in connections {
        summaries.push(ConnectionSummary { connection_id, protocol: connection.lock().await.protocol() });
    }
    Ok(summaries)
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
    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
    connection.file_system().list_dir(&request.path).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn sync_preview(
    request: SyncPreviewRequest,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncPreview, String> {
    let local =
        filter_snapshot(collect_local_snapshot(Path::new(&request.local_directory))?, &request.exclusions);
    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
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
    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
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

    drop(connection);
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
    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
    connection.file_system().create_dir(&request.path).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remote_rename(request: RenameRequest, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
    connection
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

fn atomic_upload_path(destination: &str, transfer_id: &str) -> Result<String, String> {
    let (parent, _) = remote_parent_and_name(destination)?;
    let mut hasher = DefaultHasher::new();
    transfer_id.hash(&mut hasher);
    destination.hash(&mut hasher);
    let temporary_name = format!(".harbor-upload-{:016x}.part", hasher.finish());
    Ok(remote_child(&parent, &temporary_name))
}

fn local_download_staging_path(destination: &Path, transfer_id: &str) -> Result<PathBuf, String> {
    let parent = destination.parent().ok_or("The download destination must have a parent directory.")?;
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or("The download destination must have a valid file name.")?;
    let mut hasher = DefaultHasher::new();
    transfer_id.hash(&mut hasher);
    destination.hash(&mut hasher);
    Ok(parent.join(format!(".{name}.harbor-transfer-{:016x}.part", hasher.finish())))
}

async fn commit_local_download(
    staging: &Path,
    destination: &Path,
    expected_bytes: u64,
) -> anyhow::Result<()> {
    let actual_bytes = tokio::fs::metadata(staging)
        .await
        .map_err(|error| {
            anyhow::anyhow!("Download verification could not inspect '{}': {error}", staging.display())
        })?
        .len();
    anyhow::ensure!(
        actual_bytes == expected_bytes,
        "Download size verification failed: the transfer reported {expected_bytes} bytes, but the staged file contains {actual_bytes} bytes. The original destination was not changed."
    );
    tokio::fs::rename(staging, destination).await.map_err(|error| {
        anyhow::anyhow!("The verified download could not replace '{}': {error}", destination.display())
    })
}

fn verify_uploaded_size(protocol: &str, path: &str, actual: u64, expected: u64) -> anyhow::Result<()> {
    anyhow::ensure!(
        actual == expected,
        "{protocol} atomic upload verification failed for '{path}': expected {expected} bytes, but the server reported {actual} bytes. The original destination was not changed."
    );
    Ok(())
}

async fn verify_remote_file_size(
    file_system: &mut dyn RemoteFileSystem,
    path: &str,
    expected: u64,
) -> anyhow::Result<()> {
    let (parent, name) = remote_parent_and_name(path).map_err(anyhow::Error::msg)?;
    let entry = file_system
        .list_dir(&parent)
        .await?
        .into_iter()
        .find(|entry| {
            entry.name == name && matches!(entry.file_type, crate::sftp_client::FileEntryType::File)
        })
        .ok_or_else(|| {
            anyhow::anyhow!("The uploaded file was not found during transfer verification: '{path}'.")
        })?;
    verify_uploaded_size("Remote", path, entry.size, expected)
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

    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
    let file_system = connection.file_system();
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
    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
    if matches!(connection.protocol(), Protocol::CloudFtp) {
        return Err("Google Cloud FTP does not support changing POSIX permissions, owner, group, or modification time. Access is controlled by Cloud Storage IAM.".to_string());
    }
    connection
        .file_system()
        .set_metadata(&request.path, request.permissions, modified, request.owner_id, request.group_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remote_delete(request: DeleteRequest, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
    let file_system = connection.file_system();
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

    let connection = state.connection(&request.connection_id).await.ok_or("Connection not found.")?;
    let mut connection = connection.lock().await;
    let file_system = connection.file_system();

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
) -> Result<TransferOutcome, String> {
    let limits = request.limits();
    let resume_from = request.resume_from.unwrap_or(0);
    let transfer_id = request.transfer_id.unwrap_or_else(|| "single-upload".to_string());
    let started = std::time::Instant::now();
    let total_bytes = tokio::fs::metadata(&request.local_path).await.map(|value| value.len()).unwrap_or(0);
    let conflict_policy = request.conflict_policy.as_deref().unwrap_or("ask");
    if !matches!(conflict_policy, "ask" | "overwrite" | "skip" | "rename") {
        return Err("Invalid transfer conflict policy.".to_string());
    }
    state.bookmarks.save_transfer_job(&TransferJob {
        id: transfer_id.clone(),
        connection_id: request.connection_id.clone(),
        name: transfer_display_name(request.name.as_deref(), &request.remote_path),
        direction: "Upload".to_string(),
        local_path: request.local_path.clone(),
        remote_path: request.remote_path.clone(),
        status: "Queued".to_string(),
        detail: String::new(),
        transferred_bytes: resume_from,
        total_bytes,
        retry_count: 0,
        conflict_policy: conflict_policy.to_string(),
        is_directory: false,
        updated_at: String::new(),
    })?;
    let control = Arc::new(TransferControl::new());
    state.transfer_controls.lock().await.insert(transfer_id.clone(), control.clone());
    let _ = app.emit(
        "transfer://file-progress",
        FileTransferProgress {
            transfer_id: transfer_id.clone(),
            transferred_bytes: resume_from,
            total_bytes,
            elapsed_ms: started.elapsed().as_millis() as u64,
            status: "queued".to_string(),
        },
    );
    let permit = state
        .transfer_scheduler
        .acquire_cancellable(
            &request.connection_id,
            limits.global_concurrency,
            limits.connection_concurrency,
            &control,
        )
        .await;
    let _permit = match permit {
        Ok(permit) => permit,
        Err(error) => {
            state.transfer_controls.lock().await.remove(&transfer_id);
            return Err(error);
        }
    };
    if let Err(error) = control.wait_until_running().await {
        state.transfer_controls.lock().await.remove(&transfer_id);
        state.bookmarks.set_transfer_job_status(&transfer_id, "Cancelled", &error)?;
        return Err(error);
    }
    state.bookmarks.set_transfer_job_status(&transfer_id, "Running", "")?;
    let bandwidth = TransferBandwidth::new(state.bandwidth_limiter.clone(), &request.connection_id, limits);
    let progress_store = state.bookmarks.clone();
    let persisted_progress = Arc::new(AtomicU64::new(resume_from / TRANSFER_PROGRESS_PERSIST_INTERVAL));
    let Some(connection) = state.connection(&request.connection_id).await else {
        state.transfer_controls.lock().await.remove(&transfer_id);
        state.bookmarks.set_transfer_job_status(&transfer_id, "Failed", "Connection not found.")?;
        return Err("Connection not found.".to_string());
    };
    let connection_result = {
        let connection = connection.lock().await;
        connection.duplicate_for_transfer().await.map_err(|error| error.to_string())
    };
    let mut connection = match connection_result {
        Ok(connection) => connection,
        Err(error) => {
            state.transfer_controls.lock().await.remove(&transfer_id);
            state.bookmarks.set_transfer_job_status(&transfer_id, "Failed", &error)?;
            return Err(error);
        }
    };
    let is_google_drive_upload = matches!(&connection, RemoteConnection::GoogleDrive(_));
    let uses_sha256_verification = matches!(&connection, RemoteConnection::S3(_));
    let retry_count_base = state
        .bookmarks
        .transfer_job_checkpoint(&transfer_id)?
        .map(|(_, _, retry_count)| retry_count)
        .unwrap_or(0);
    let mut automatic_retry = 0;
    let mut attempt_resume_from = resume_from;
    let result = loop {
        bandwidth.reset(attempt_resume_from);
        let control = control.clone();
        let progress_store = progress_store.clone();
        let persisted_progress = persisted_progress.clone();
        let bandwidth = bandwidth.clone();
        let completion_bandwidth = bandwidth.clone();
        let result = match &mut connection {
            RemoteConnection::Sftp { client, .. } => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                client
                    .upload_file_resumable_with_progress(
                        &request.local_path,
                        &request.remote_path,
                        attempt_resume_from,
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
            RemoteConnection::Ftp { client, .. } => {
                client
                    .upload_file_resumable(&request.local_path, &request.remote_path, attempt_resume_from)
                    .await
            }
            RemoteConnection::WebDav(client) => {
                async {
                    let temporary_path =
                        atomic_upload_path(&request.remote_path, &transfer_id).map_err(anyhow::Error::msg)?;
                    if attempt_resume_from > 0 {
                        let _ = state.bookmarks.update_transfer_job_progress(&transfer_id, 0, total_bytes);
                    }
                    let bytes = client.upload_file(&request.local_path, &temporary_path).await?;
                    let remote_size = client.file_size(&temporary_path).await?;
                    verify_uploaded_size("WebDAV", &temporary_path, remote_size, total_bytes)?;
                    client.atomic_replace(&temporary_path, &request.remote_path).await?;
                    Ok(bytes)
                }
                .await
            }
            RemoteConnection::S3(client) => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                let resume_state = state.bookmarks.s3_multipart_state(&transfer_id)?.and_then(|json| {
                    match serde_json::from_str::<S3MultipartState>(&json) {
                        Ok(value) => Some(value),
                        Err(error) => {
                            tracing::warn!("Discarding invalid saved S3 multipart state: {error}");
                            let _ = state.bookmarks.delete_s3_multipart_state(&transfer_id);
                            None
                        }
                    }
                });
                let state_store = state.bookmarks.clone();
                let state_id = transfer_id.clone();
                client
                    .upload_file_resumable_with_progress(
                        &request.local_path,
                        &request.remote_path,
                        resume_state,
                        true,
                        move |multipart| {
                            let json = serde_json::to_string(multipart)?;
                            state_store.save_s3_multipart_state(&state_id, &json).map_err(anyhow::Error::msg)
                        },
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
            RemoteConnection::Samba(client) => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                client
                    .upload_file_with_progress(
                        &request.local_path,
                        &request.remote_path,
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
            RemoteConnection::GoogleDrive(client) => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                let session_key = format!("google:upload-session:{transfer_id}");
                let resume_state = match secret_store::lookup(&session_key)
                    .map_err(|error| error.to_string())?
                {
                    SecretLookup::Value(json) => {
                        match serde_json::from_str::<GoogleDriveUploadState>(&json) {
                            Ok(value) => Some(value),
                            Err(error) => {
                                tracing::warn!("Discarding invalid saved Google Drive upload state: {error}");
                                secret_store::remove_ephemeral(&session_key)
                                    .map_err(|error| error.to_string())?;
                                None
                            }
                        }
                    }
                    SecretLookup::Missing | SecretLookup::Removed => None,
                };
                let state_key = session_key.clone();
                client
                    .upload_file_resumable_with_progress(
                        &request.local_path,
                        &request.remote_path,
                        resume_state,
                        true,
                        move |upload_state| {
                            secret_store::store(&state_key, &serde_json::to_string(upload_state)?)
                        },
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
        };
        if let Ok(bytes) = &result {
            completion_bandwidth.progress(*bytes).await;
        }
        let result = match result {
            Ok(bytes) if bytes != total_bytes => Err(anyhow::anyhow!(
                "Upload size verification failed: expected {total_bytes} bytes, but the transfer reported {bytes} bytes."
            )),
            Ok(bytes) if !is_google_drive_upload => {
                verify_remote_file_size(connection.file_system(), &request.remote_path, total_bytes)
                    .await
                    .map(|_| bytes)
            }
            other => other,
        };
        match result {
            Err(error)
                if automatic_retry < limits.automatic_retries && is_retryable_transfer_error(&error) =>
            {
                automatic_retry += 1;
                let detail = format!(
                    "Temporary transfer failure. Reconnecting (attempt {automatic_retry}/{}): {error}",
                    limits.automatic_retries
                );
                state.bookmarks.set_transfer_job_retry(
                    &transfer_id,
                    retry_count_base.saturating_add(automatic_retry),
                    &detail,
                )?;
                let _ = app.emit(
                    "transfer://file-progress",
                    FileTransferProgress {
                        transfer_id: transfer_id.clone(),
                        transferred_bytes: attempt_resume_from,
                        total_bytes,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        status: "reconnecting".to_string(),
                    },
                );
                tokio::time::sleep(retry_backoff(automatic_retry - 1)).await;
                if let Err(reconnect_error) = connection.reconnect().await {
                    tracing::warn!("Automatic reconnect attempt {automatic_retry} failed: {reconnect_error}");
                }
                attempt_resume_from = state
                    .bookmarks
                    .transfer_job_checkpoint(&transfer_id)?
                    .map(|(transferred, _, _)| transferred)
                    .unwrap_or(attempt_resume_from);
            }
            other => break other,
        }
    };
    drop(connection);
    state.transfer_controls.lock().await.remove(&transfer_id);
    let bytes = match result {
        Ok(bytes) => {
            if is_google_drive_upload {
                let session_key = format!("google:upload-session:{transfer_id}");
                if matches!(secret_store::lookup(&session_key), Ok(SecretLookup::Value(_))) {
                    secret_store::remove_ephemeral(&session_key).map_err(|error| error.to_string())?;
                }
            }
            state.bookmarks.delete_transfer_job(&transfer_id)?;
            bytes
        }
        Err(error) => {
            let detail = error.to_string();
            state.bookmarks.set_transfer_job_status(&transfer_id, "Failed", &detail)?;
            return Err(detail);
        }
    };
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
    Ok(TransferOutcome {
        bytes,
        verification: if uses_sha256_verification { "sha256" } else { "size" }.to_string(),
    })
}

#[tauri::command]
pub async fn transfer_download(
    request: TransferRequest,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<TransferOutcome, String> {
    let limits = request.limits();
    let transfer_id = request.transfer_id.unwrap_or_else(|| "single-download".to_string());
    let destination_path = PathBuf::from(&request.local_path);
    let staging_path = local_download_staging_path(&destination_path, &transfer_id)?;
    let staging_path_text = staging_path.to_string_lossy().into_owned();
    let staged_bytes = tokio::fs::metadata(&staging_path).await.map(|metadata| metadata.len()).unwrap_or(0);
    let resume_from = request.resume_from.unwrap_or(0).min(staged_bytes);
    let started = std::time::Instant::now();
    let conflict_policy = request.conflict_policy.as_deref().unwrap_or("ask");
    if !matches!(conflict_policy, "ask" | "overwrite" | "skip" | "rename") {
        return Err("Invalid transfer conflict policy.".to_string());
    }
    state.bookmarks.save_transfer_job(&TransferJob {
        id: transfer_id.clone(),
        connection_id: request.connection_id.clone(),
        name: transfer_display_name(request.name.as_deref(), &request.remote_path),
        direction: "Download".to_string(),
        local_path: request.local_path.clone(),
        remote_path: request.remote_path.clone(),
        status: "Queued".to_string(),
        detail: String::new(),
        transferred_bytes: resume_from,
        total_bytes: 0,
        retry_count: 0,
        conflict_policy: conflict_policy.to_string(),
        is_directory: false,
        updated_at: String::new(),
    })?;
    let control = Arc::new(TransferControl::new());
    state.transfer_controls.lock().await.insert(transfer_id.clone(), control.clone());
    let _ = app.emit(
        "transfer://file-progress",
        FileTransferProgress {
            transfer_id: transfer_id.clone(),
            transferred_bytes: resume_from,
            total_bytes: 0,
            elapsed_ms: started.elapsed().as_millis() as u64,
            status: "queued".to_string(),
        },
    );
    let permit = state
        .transfer_scheduler
        .acquire_cancellable(
            &request.connection_id,
            limits.global_concurrency,
            limits.connection_concurrency,
            &control,
        )
        .await;
    let _permit = match permit {
        Ok(permit) => permit,
        Err(error) => {
            state.transfer_controls.lock().await.remove(&transfer_id);
            return Err(error);
        }
    };
    if let Err(error) = control.wait_until_running().await {
        state.transfer_controls.lock().await.remove(&transfer_id);
        state.bookmarks.set_transfer_job_status(&transfer_id, "Cancelled", &error)?;
        return Err(error);
    }
    state.bookmarks.set_transfer_job_status(&transfer_id, "Running", "")?;
    let bandwidth = TransferBandwidth::new(state.bandwidth_limiter.clone(), &request.connection_id, limits);
    let progress_store = state.bookmarks.clone();
    let persisted_progress = Arc::new(AtomicU64::new(resume_from / TRANSFER_PROGRESS_PERSIST_INTERVAL));
    let Some(connection) = state.connection(&request.connection_id).await else {
        state.transfer_controls.lock().await.remove(&transfer_id);
        state.bookmarks.set_transfer_job_status(&transfer_id, "Failed", "Connection not found.")?;
        return Err("Connection not found.".to_string());
    };
    let connection_result = {
        let connection = connection.lock().await;
        connection.duplicate_for_transfer().await.map_err(|error| error.to_string())
    };
    let mut connection = match connection_result {
        Ok(connection) => connection,
        Err(error) => {
            state.transfer_controls.lock().await.remove(&transfer_id);
            state.bookmarks.set_transfer_job_status(&transfer_id, "Failed", &error)?;
            return Err(error);
        }
    };
    let is_google_drive_download = matches!(&connection, RemoteConnection::GoogleDrive(_));
    let uses_sha256_verification = matches!(&connection, RemoteConnection::S3(_));
    let retry_count_base = state
        .bookmarks
        .transfer_job_checkpoint(&transfer_id)?
        .map(|(_, _, retry_count)| retry_count)
        .unwrap_or(0);
    let mut automatic_retry = 0;
    let mut attempt_resume_from = resume_from;
    let result = loop {
        bandwidth.reset(attempt_resume_from);
        let control = control.clone();
        let progress_store = progress_store.clone();
        let persisted_progress = persisted_progress.clone();
        let bandwidth = bandwidth.clone();
        let completion_bandwidth = bandwidth.clone();
        let result = match &mut connection {
            RemoteConnection::Sftp { client, .. } => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                client
                    .download_file_resumable_with_progress(
                        &request.remote_path,
                        &staging_path_text,
                        attempt_resume_from,
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
            RemoteConnection::Ftp { client, .. } => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                client
                    .download_file_resumable_with_progress(
                        &request.remote_path,
                        &staging_path_text,
                        attempt_resume_from,
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
            RemoteConnection::WebDav(client) => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                client
                    .download_file_resumable_with_progress(
                        &request.remote_path,
                        &staging_path_text,
                        attempt_resume_from,
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
            RemoteConnection::S3(client) => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                client
                    .download_file_with_progress(
                        &request.remote_path,
                        &staging_path_text,
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
            RemoteConnection::Samba(client) => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                client
                    .download_file_with_progress(
                        &request.remote_path,
                        &staging_path_text,
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
            RemoteConnection::GoogleDrive(client) => {
                let event_app = app.clone();
                let event_id = transfer_id.clone();
                client
                    .download_file_with_progress(
                        &request.remote_path,
                        &staging_path_text,
                        move |done, total| {
                            let event_app = event_app.clone();
                            let event_id = event_id.clone();
                            let control = control.clone();
                            let progress_store = progress_store.clone();
                            let persisted_progress = persisted_progress.clone();
                            let bandwidth = bandwidth.clone();
                            async move {
                                control.wait_until_running().await.map_err(anyhow::Error::msg)?;
                                bandwidth.progress(done).await;
                                persist_progress(
                                    &progress_store,
                                    &event_id,
                                    done,
                                    total,
                                    &persisted_progress,
                                );
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
                        },
                    )
                    .await
            }
        };
        if let Ok(bytes) = &result {
            completion_bandwidth.progress(*bytes).await;
        }
        let result = match result {
            Ok(bytes) if !is_google_drive_download => {
                verify_remote_file_size(connection.file_system(), &request.remote_path, bytes)
                    .await
                    .map(|_| bytes)
            }
            other => other,
        };
        let result = match result {
            Ok(bytes) => commit_local_download(&staging_path, &destination_path, bytes).await.map(|_| bytes),
            other => other,
        };
        match result {
            Err(error)
                if automatic_retry < limits.automatic_retries && is_retryable_transfer_error(&error) =>
            {
                automatic_retry += 1;
                let detail = format!(
                    "Temporary transfer failure. Reconnecting (attempt {automatic_retry}/{}): {error}",
                    limits.automatic_retries
                );
                state.bookmarks.set_transfer_job_retry(
                    &transfer_id,
                    retry_count_base.saturating_add(automatic_retry),
                    &detail,
                )?;
                let checkpoint = state.bookmarks.transfer_job_checkpoint(&transfer_id)?;
                let reconnect_bytes = checkpoint
                    .as_ref()
                    .map(|(transferred, _, _)| *transferred)
                    .unwrap_or(attempt_resume_from);
                let reconnect_total = checkpoint.map(|(_, total, _)| total).unwrap_or(0);
                let _ = app.emit(
                    "transfer://file-progress",
                    FileTransferProgress {
                        transfer_id: transfer_id.clone(),
                        transferred_bytes: reconnect_bytes,
                        total_bytes: reconnect_total,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        status: "reconnecting".to_string(),
                    },
                );
                tokio::time::sleep(retry_backoff(automatic_retry - 1)).await;
                if let Err(reconnect_error) = connection.reconnect().await {
                    tracing::warn!("Automatic reconnect attempt {automatic_retry} failed: {reconnect_error}");
                }
                attempt_resume_from = reconnect_bytes;
            }
            other => break other,
        }
    };
    drop(connection);
    state.transfer_controls.lock().await.remove(&transfer_id);
    let bytes = match result {
        Ok(bytes) => {
            state.bookmarks.delete_transfer_job(&transfer_id)?;
            bytes
        }
        Err(error) => {
            let detail = error.to_string();
            state.bookmarks.set_transfer_job_status(&transfer_id, "Failed", &detail)?;
            return Err(detail);
        }
    };
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
    Ok(TransferOutcome {
        bytes,
        verification: if uses_sha256_verification { "sha256" } else { "size" }.to_string(),
    })
}

#[tauri::command]
pub async fn transfer_upload_directory(
    request: DirectoryTransferRequest,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let limits = request.limits();
    let root = PathBuf::from(&request.local_directory);
    if !root.is_dir() {
        return Err("The selected local path is not a directory.".to_string());
    }
    let mut entries = Vec::new();
    collect_local_entries(&root, &root, &mut entries)?;
    let total_files = entries.iter().filter(|entry| matches!(entry, LocalEntry::File(_, _))).count();
    let total_bytes = entries
        .iter()
        .filter_map(|entry| match entry {
            LocalEntry::File(path, _) => std::fs::metadata(path).ok().map(|value| value.len()),
            LocalEntry::Directory(_) => None,
        })
        .sum();
    let conflict_policy = request.conflict_policy.as_deref().unwrap_or("ask");
    if !matches!(conflict_policy, "ask" | "overwrite" | "skip" | "rename") {
        return Err("Invalid transfer conflict policy.".to_string());
    }
    state.bookmarks.save_transfer_job(&TransferJob {
        id: request.transfer_id.clone(),
        connection_id: request.connection_id.clone(),
        name: transfer_display_name(request.name.as_deref(), &request.remote_directory),
        direction: "Upload".to_string(),
        local_path: request.local_directory.clone(),
        remote_path: request.remote_directory.clone(),
        status: "Queued".to_string(),
        detail: String::new(),
        transferred_bytes: 0,
        total_bytes,
        retry_count: 0,
        conflict_policy: conflict_policy.to_string(),
        is_directory: true,
        updated_at: String::new(),
    })?;
    let mut completed_files = 0;
    let mut transferred_bytes = 0u64;
    let control = Arc::new(TransferControl::new());
    state.transfer_controls.lock().await.insert(request.transfer_id.clone(), control.clone());
    let _ = app.emit(
        "transfer://progress",
        DirectoryTransferProgress {
            transfer_id: request.transfer_id.clone(),
            completed_files: 0,
            total_files,
            current_path: request.remote_directory.clone(),
            status: "queued".to_string(),
        },
    );
    let permit = state
        .transfer_scheduler
        .acquire_cancellable(
            &request.connection_id,
            limits.global_concurrency,
            limits.connection_concurrency,
            &control,
        )
        .await;
    let _permit = match permit {
        Ok(permit) => permit,
        Err(error) => {
            state.transfer_controls.lock().await.remove(&request.transfer_id);
            return Err(error);
        }
    };
    if let Err(error) = control.wait_until_running().await {
        state.transfer_controls.lock().await.remove(&request.transfer_id);
        state.bookmarks.set_transfer_job_status(&request.transfer_id, "Cancelled", &error)?;
        return Err(error);
    }
    state.bookmarks.set_transfer_job_status(&request.transfer_id, "Running", "")?;
    let bandwidth = TransferBandwidth::new(state.bandwidth_limiter.clone(), &request.connection_id, limits);
    let Some(connection) = state.connection(&request.connection_id).await else {
        state.transfer_controls.lock().await.remove(&request.transfer_id);
        state.bookmarks.set_transfer_job_status(&request.transfer_id, "Failed", "Connection not found.")?;
        return Err("Connection not found.".to_string());
    };
    let connection_result = {
        let connection = connection.lock().await;
        connection.duplicate_for_transfer().await.map_err(|error| error.to_string())
    };
    let mut connection = match connection_result {
        Ok(connection) => connection,
        Err(error) => {
            state.transfer_controls.lock().await.remove(&request.transfer_id);
            state.bookmarks.set_transfer_job_status(&request.transfer_id, "Failed", &error)?;
            return Err(error);
        }
    };
    let retry_count_base = state
        .bookmarks
        .transfer_job_checkpoint(&request.transfer_id)?
        .map(|(_, _, retry_count)| retry_count)
        .unwrap_or(0);

    // The selected folder itself must exist before empty folders or nested
    // files can be transferred. An existing destination is harmless.
    let _ = connection.file_system().create_dir(&request.remote_directory).await;

    for entry in entries {
        if let Err(error) = control.wait_until_running().await {
            state.transfer_controls.lock().await.remove(&request.transfer_id);
            state.bookmarks.set_transfer_job_status(&request.transfer_id, "Failed", &error)?;
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
                let expected_size =
                    tokio::fs::metadata(&local_path).await.map(|value| value.len()).unwrap_or(0);
                let mut automatic_retry = 0;
                let upload_result = loop {
                    bandwidth.reset(0);
                    let control = control.clone();
                    let bandwidth = bandwidth.clone();
                    let completion_bandwidth = bandwidth.clone();
                    let upload_result = match &mut connection {
                        RemoteConnection::WebDav(client) => {
                            async {
                                let temporary_path = atomic_upload_path(&remote_path, &request.transfer_id)
                                    .map_err(anyhow::Error::msg)?;
                                let bytes = client.upload_file(&local_path_string, &temporary_path).await?;
                                let remote_size = client.file_size(&temporary_path).await?;
                                verify_uploaded_size("WebDAV", &temporary_path, remote_size, expected_size)?;
                                client.atomic_replace(&temporary_path, &remote_path).await?;
                                Ok(bytes)
                            }
                            .await
                        }
                        RemoteConnection::S3(client) => {
                            let file_control = control.clone();
                            client
                                .upload_file_with_progress(
                                    &local_path_string,
                                    &remote_path,
                                    move |done, _| {
                                        let file_control = file_control.clone();
                                        let bandwidth = bandwidth.clone();
                                        async move {
                                            file_control
                                                .wait_until_running()
                                                .await
                                                .map_err(anyhow::Error::msg)?;
                                            bandwidth.progress(done).await;
                                            Ok(())
                                        }
                                    },
                                )
                                .await
                        }
                        RemoteConnection::Samba(client) => {
                            let file_control = control.clone();
                            client
                                .upload_file_with_progress(
                                    &local_path_string,
                                    &remote_path,
                                    move |done, _| {
                                        let file_control = file_control.clone();
                                        let bandwidth = bandwidth.clone();
                                        async move {
                                            file_control
                                                .wait_until_running()
                                                .await
                                                .map_err(anyhow::Error::msg)?;
                                            bandwidth.progress(done).await;
                                            Ok(())
                                        }
                                    },
                                )
                                .await
                        }
                        RemoteConnection::GoogleDrive(client) => {
                            let file_control = control.clone();
                            client
                                .upload_file_with_progress(
                                    &local_path_string,
                                    &remote_path,
                                    move |done, _| {
                                        let file_control = file_control.clone();
                                        let bandwidth = bandwidth.clone();
                                        async move {
                                            file_control
                                                .wait_until_running()
                                                .await
                                                .map_err(anyhow::Error::msg)?;
                                            bandwidth.progress(done).await;
                                            Ok(())
                                        }
                                    },
                                )
                                .await
                        }
                        _ => connection.file_system().upload_file(&local_path_string, &remote_path).await,
                    };
                    if let Ok(bytes) = &upload_result {
                        completion_bandwidth.progress(*bytes).await;
                    }
                    let upload_result = match upload_result {
                        Ok(bytes) if bytes != expected_size => Err(anyhow::anyhow!(
                            "Upload size verification failed for '{}': expected {expected_size} bytes, but the transfer reported {bytes} bytes.",
                            relative_path.display()
                        )),
                        Ok(bytes) if !matches!(&connection, RemoteConnection::GoogleDrive(_)) => {
                            verify_remote_file_size(connection.file_system(), &remote_path, expected_size)
                                .await
                                .map(|_| bytes)
                        }
                        other => other,
                    };
                    match upload_result {
                        Err(error)
                            if automatic_retry < limits.automatic_retries
                                && is_retryable_transfer_error(&error) =>
                        {
                            automatic_retry += 1;
                            let detail = format!(
                                "Temporary transfer failure for '{}'. Reconnecting (attempt {automatic_retry}/{}): {error}",
                                relative_path.display(),
                                limits.automatic_retries
                            );
                            state.bookmarks.set_transfer_job_retry(
                                &request.transfer_id,
                                retry_count_base.saturating_add(automatic_retry),
                                &detail,
                            )?;
                            let _ = app.emit(
                                "transfer://progress",
                                DirectoryTransferProgress {
                                    transfer_id: request.transfer_id.clone(),
                                    completed_files,
                                    total_files,
                                    current_path: relative_path.to_string_lossy().to_string(),
                                    status: "reconnecting".to_string(),
                                },
                            );
                            tokio::time::sleep(retry_backoff(automatic_retry - 1)).await;
                            if let Err(reconnect_error) = connection.reconnect().await {
                                tracing::warn!(
                                    "Automatic directory-transfer reconnect attempt {automatic_retry} failed: {reconnect_error}"
                                );
                            }
                        }
                        other => break other,
                    }
                };
                match upload_result {
                    Ok(bytes) => {
                        transferred_bytes = transferred_bytes.saturating_add(bytes);
                        state.bookmarks.update_transfer_job_progress(
                            &request.transfer_id,
                            transferred_bytes,
                            total_bytes,
                        )?;
                    }
                    Err(error) => {
                        state.transfer_controls.lock().await.remove(&request.transfer_id);
                        state.bookmarks.set_transfer_job_status(
                            &request.transfer_id,
                            "Failed",
                            &error.to_string(),
                        )?;
                        return Err(error.to_string());
                    }
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
    state.bookmarks.delete_transfer_job(&request.transfer_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        atomic_upload_path, commit_local_download, content_hash, effective_transfer_limits,
        is_retryable_transfer_error, local_directory_list, local_download_staging_path,
        parse_remote_modified, reject_symlink_ancestors, remote_child_path, remote_parent_and_name,
        remote_replace_target, reserve_bandwidth, retry_backoff, safe_relative_path, Protocol,
        TransferControl, TransferScheduler,
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

    #[test]
    fn transfer_retry_policy_uses_bounded_exponential_backoff() {
        assert_eq!(retry_backoff(0), std::time::Duration::from_millis(500));
        assert_eq!(retry_backoff(1), std::time::Duration::from_secs(1));
        assert_eq!(retry_backoff(2), std::time::Duration::from_secs(2));
        assert_eq!(retry_backoff(3), std::time::Duration::from_secs(4));
        assert_eq!(retry_backoff(30), std::time::Duration::from_secs(4));
    }

    #[test]
    fn transfer_limits_are_bounded_and_allow_disabling_retry_and_bandwidth() {
        let limits = effective_transfer_limits(99, Some(0), u64::MAX, Some(0), Some(0));
        assert_eq!(limits.global_concurrency, 16);
        assert_eq!(limits.connection_concurrency, 1);
        assert_eq!(limits.global_bandwidth_bps, 10 * 1024 * 1024 * 1024);
        assert_eq!(limits.connection_bandwidth_bps, 0);
        assert_eq!(limits.automatic_retries, 0);
    }

    #[test]
    fn aggregate_bandwidth_reservations_share_one_schedule() {
        let now = std::time::Instant::now();
        let mut next = None;
        let first = reserve_bandwidth(&mut next, now, 1024, 1024);
        let second = reserve_bandwidth(&mut next, now, 1024, 1024);
        assert_eq!(first.duration_since(now), std::time::Duration::from_secs(1));
        assert_eq!(second.duration_since(now), std::time::Duration::from_secs(2));
    }

    #[tokio::test]
    async fn transfer_scheduler_releases_queued_work_when_capacity_returns() {
        let scheduler = TransferScheduler::new();
        let first = scheduler.acquire("bookmark-a", 1, 1).await;
        let waiting_scheduler = scheduler.clone();
        let mut waiting = tokio::spawn(async move { waiting_scheduler.acquire("bookmark-b", 1, 1).await });
        assert!(tokio::time::timeout(std::time::Duration::from_millis(20), &mut waiting).await.is_err());
        drop(first);
        let second = tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .expect("queued transfer should start")
            .expect("scheduler task should not panic");
        drop(second);
    }

    #[tokio::test]
    async fn transfer_scheduler_applies_connection_limits_independently() {
        let scheduler = TransferScheduler::new();
        let first = scheduler.acquire("bookmark-a", 2, 1).await;
        let other = scheduler.acquire("bookmark-b", 2, 1).await;
        let waiting_scheduler = scheduler.clone();
        let mut same_bookmark =
            tokio::spawn(async move { waiting_scheduler.acquire("bookmark-a", 2, 1).await });
        assert!(tokio::time::timeout(std::time::Duration::from_millis(20), &mut same_bookmark)
            .await
            .is_err());
        drop(first);
        let resumed = tokio::time::timeout(std::time::Duration::from_secs(1), same_bookmark)
            .await
            .expect("connection capacity should be released")
            .expect("scheduler task should not panic");
        drop(resumed);
        drop(other);
    }

    #[tokio::test]
    async fn queued_transfer_can_be_cancelled_without_waiting_for_capacity() {
        let scheduler = TransferScheduler::new();
        let first = scheduler.acquire("bookmark-a", 1, 1).await;
        let control = Arc::new(TransferControl::new());
        let waiting_scheduler = scheduler.clone();
        let waiting_control = control.clone();
        let waiting = tokio::spawn(async move {
            waiting_scheduler.acquire_cancellable("bookmark-b", 1, 1, &waiting_control).await
        });
        tokio::task::yield_now().await;
        control.cancelled.store(true, Ordering::Release);
        scheduler.changed.notify_waiters();
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .expect("cancelled queue wait should finish")
            .expect("scheduler task should not panic");
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("cancelled transfer must not get a permit"),
        };
        assert_eq!(error, "Transfer cancelled.");
        drop(first);
    }

    #[test]
    fn transfer_retry_policy_separates_transient_and_permanent_failures() {
        for message in [
            "operation timed out",
            "connection reset by peer",
            "HTTP 503 Service Unavailable",
            "429 too many requests",
            "unexpected EOF",
        ] {
            assert!(is_retryable_transfer_error(&anyhow::anyhow!(message)), "{message}");
        }
        let wrapped = anyhow::anyhow!("connection reset by peer").context("upload failed");
        assert!(is_retryable_transfer_error(&wrapped));
        for message in [
            "authentication failed",
            "HTTP 403 Forbidden",
            "permission denied",
            "file not found",
            "checksum verification failed",
            "transfer cancelled",
            "connection not found",
        ] {
            assert!(!is_retryable_transfer_error(&anyhow::anyhow!(message)), "{message}");
        }
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
    fn cloud_ftp_protocol_uses_a_stable_camel_case_wire_value() {
        let protocol: Protocol = serde_json::from_str("\"cloudFtp\"").expect("deserialize Cloud FTP");
        assert_eq!(protocol, Protocol::CloudFtp);
        assert_eq!(serde_json::to_string(&protocol).unwrap(), "\"cloudFtp\"");
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
    fn atomic_upload_paths_are_hidden_stable_and_colocated() {
        let first = atomic_upload_path("/docs/港便り.txt", "transfer-42").unwrap();
        let second = atomic_upload_path("/docs/港便り.txt", "transfer-42").unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with("/docs/.harbor-upload-"));
        assert!(first.ends_with(".part"));
        assert_ne!(first, atomic_upload_path("/docs/港便り.txt", "different-transfer").unwrap());
        assert_ne!(first, atomic_upload_path("/docs/別のファイル.txt", "transfer-42").unwrap());
        assert!(atomic_upload_path("relative.txt", "transfer-42").is_err());
    }

    #[tokio::test]
    async fn verified_download_replaces_destination_only_after_size_validation() {
        let workspace = tempfile::tempdir().unwrap();
        let destination = workspace.path().join("report.txt");
        let staging = local_download_staging_path(&destination, "transfer-42").unwrap();
        assert_eq!(staging.parent(), destination.parent());
        assert!(staging.file_name().unwrap().to_string_lossy().ends_with(".part"));
        assert_eq!(staging, local_download_staging_path(&destination, "transfer-42").unwrap());

        tokio::fs::write(&destination, b"original").await.unwrap();
        tokio::fs::write(&staging, b"partial").await.unwrap();
        let mismatch = commit_local_download(&staging, &destination, 99).await.unwrap_err();
        assert!(mismatch.to_string().contains("original destination was not changed"));
        assert_eq!(tokio::fs::read(&destination).await.unwrap(), b"original");
        assert_eq!(tokio::fs::read(&staging).await.unwrap(), b"partial");

        commit_local_download(&staging, &destination, 7).await.unwrap();
        assert_eq!(tokio::fs::read(&destination).await.unwrap(), b"partial");
        assert!(!staging.exists());
    }

    #[tokio::test]
    async fn unavailable_staging_file_preserves_existing_destination() {
        let workspace = tempfile::tempdir().unwrap();
        let destination = workspace.path().join("important.txt");
        let staging = local_download_staging_path(&destination, "disk-full-transfer").unwrap();
        tokio::fs::write(&destination, b"keep me").await.unwrap();

        let error = commit_local_download(&staging, &destination, 1024).await.unwrap_err();
        assert!(error.to_string().contains("could not inspect"));
        assert_eq!(tokio::fs::read(&destination).await.unwrap(), b"keep me");
    }

    #[tokio::test]
    async fn lists_local_directories_with_stable_paths_and_directory_first_order() {
        let workspace = tempfile::tempdir().unwrap();
        let child = workspace.path().join("Folder");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(workspace.path().join("document.txt"), b"hello").unwrap();

        let listing = local_directory_list(workspace.path().to_string_lossy().into_owned()).await.unwrap();
        assert_eq!(listing.path, workspace.path().canonicalize().unwrap().to_string_lossy());
        assert_eq!(listing.entries.len(), 2);
        assert_eq!(listing.entries[0].name, "Folder");
        assert_eq!(listing.entries[0].kind, "Directory");
        assert_eq!(listing.entries[1].name, "document.txt");
        assert_eq!(listing.entries[1].size, 5);
        assert!(listing.parent.is_some());

        let error =
            local_directory_list(workspace.path().join("document.txt").to_string_lossy().into_owned())
                .await
                .unwrap_err();
        assert!(error.contains("not a directory"));
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

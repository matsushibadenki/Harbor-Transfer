use crate::secret_store::{self, SecretLookup};
use crate::sftp_client::{FileEntry, FileEntryType};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use futures_util::StreamExt;
use rand::{distributions::Alphanumeric, Rng};
use reqwest::{header, Client, Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

const DRIVE_API: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const AUTHORIZATION_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const USERINFO_ENDPOINT: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive";
const GOOGLE_KEYCHAIN_ACCOUNT: &str = "google-drive-oauth";
const GOOGLE_CREDENTIALS_KEYCHAIN_ACCOUNT: &str = "google-drive-client-credentials";
const GOOGLE_AUTHORIZATION_VAULT_KEY: &str = "google:authorization";
const GOOGLE_CREDENTIALS_VAULT_KEY: &str = "google:client-credentials";
const MAX_LIST_ENTRIES: usize = 100_000;
const DRIVE_PATH_PREFIX: &str = "~gdrive~";

#[derive(Default)]
struct GoogleKeychainCache {
    credentials_loaded: bool,
    credentials: Option<StoredGoogleClientCredentials>,
    authorization_loaded: bool,
    authorization: Option<StoredGoogleAuthorization>,
}

static GOOGLE_KEYCHAIN_CACHE: OnceLock<Mutex<GoogleKeychainCache>> = OnceLock::new();

fn keychain_cache() -> &'static Mutex<GoogleKeychainCache> {
    GOOGLE_KEYCHAIN_CACHE.get_or_init(|| Mutex::new(GoogleKeychainCache::default()))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGoogleClientCredentials {
    client_id: String,
    client_secret: String,
}

#[derive(Deserialize)]
struct GoogleCredentialsFile {
    installed: Option<GoogleInstalledCredentials>,
}

#[derive(Deserialize)]
struct GoogleInstalledCredentials {
    client_id: String,
    client_secret: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGoogleAuthorization {
    client_id: String,
    access_token: String,
    refresh_token: String,
    expires_at: u64,
    email: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleAuthorizationStatus {
    pub authorized: bool,
    pub email: Option<String>,
    pub client_matches: bool,
    pub credentials_ready: bool,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct GoogleErrorResponse {
    error: Option<GoogleErrorValue>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum GoogleErrorValue {
    Text(String),
    Detail { message: Option<String> },
}

impl GoogleErrorValue {
    fn message(self) -> Option<String> {
        match self {
            Self::Text(value) => Some(value),
            Self::Detail { message } => message,
        }
    }
}

#[derive(Deserialize)]
struct UserInfo {
    email: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    name: String,
    mime_type: String,
    size: Option<String>,
    modified_time: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFileList {
    files: Vec<DriveFile>,
    next_page_token: Option<String>,
}

pub struct GoogleDriveClient {
    http: Client,
    authorization: StoredGoogleAuthorization,
    file_cache: HashMap<String, DriveFile>,
}

impl GoogleDriveClient {
    pub async fn connect(client_id: &str, probe_path: &str) -> Result<Self> {
        validate_client_id(client_id)?;
        let authorization = load_authorization()?
            .ok_or_else(|| anyhow!("Google Drive is not authorized. Open Preferences and sign in first."))?;
        if authorization.client_id != client_id.trim() {
            bail!("The saved Google authorization belongs to a different Client ID. Authorize this Client ID in Preferences.");
        }
        let mut client = Self {
            http: Client::builder()
                .https_only(true)
                .timeout(Duration::from_secs(120))
                .user_agent("Harbor-Transfer/Google-Drive")
                .build()?,
            authorization,
            file_cache: HashMap::new(),
        };
        client.list_dir(probe_path).await?;
        Ok(client)
    }

    pub async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>> {
        let folder = self.resolve_path(path).await?;
        if folder.mime_type != "application/vnd.google-apps.folder" {
            bail!("The Google Drive path is not a folder: {path}");
        }
        let mut entries = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            self.ensure_access_token().await?;
            let query = format!("'{}' in parents and trashed = false", folder.id);
            let mut request = self
                .http
                .get(format!("{DRIVE_API}/files"))
                .bearer_auth(&self.authorization.access_token)
                .query(&[
                    ("q", query.as_str()),
                    ("spaces", "drive"),
                    ("pageSize", "1000"),
                    ("fields", "nextPageToken,files(id,name,mimeType,size,modifiedTime)"),
                    ("supportsAllDrives", "true"),
                    ("includeItemsFromAllDrives", "true"),
                ]);
            if let Some(token) = page_token.as_ref() {
                request = request.query(&[("pageToken", token)]);
            }
            let response = request.send().await?;
            let response = require_success(response, "list Google Drive files").await?;
            let page: DriveFileList = response.json().await?;
            for file in page.files {
                self.file_cache.insert(file.id.clone(), file.clone());
                let download_name = export_format(&file.mime_type)
                    .map(|(_, extension)| format!("{}.{}", file.name, extension));
                entries.push(FileEntry {
                    path_component: Some(encode_drive_path_component(&file.name, &file.id)),
                    name: file.name,
                    download_name,
                    size: file.size.as_deref().and_then(|value| value.parse().ok()).unwrap_or(0),
                    modified: file.modified_time,
                    permissions: None,
                    file_type: if file.mime_type == "application/vnd.google-apps.folder" {
                        FileEntryType::Directory
                    } else {
                        FileEntryType::File
                    },
                    owner: None,
                    group: None,
                });
            }
            if entries.len() > MAX_LIST_ENTRIES {
                bail!("Google Drive folder contains more than {MAX_LIST_ENTRIES} items.");
            }
            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        entries.sort_by(|left, right| {
            let left_file = matches!(left.file_type, FileEntryType::File);
            let right_file = matches!(right.file_type, FileEntryType::File);
            left_file.cmp(&right_file).then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> Result<u64> {
        let (parent_path, name) = split_parent_and_name(remote_path)?;
        let parent = self.resolve_path(&parent_path).await?;
        let existing = self.find_child(&parent.id, &name).await?;
        if existing.as_ref().is_some_and(|file| file.mime_type == "application/vnd.google-apps.folder") {
            bail!("A Google Drive folder already uses the name '{name}'.");
        }
        let source = PathBuf::from(local_path);
        let total = tokio::fs::metadata(&source).await?.len();
        self.ensure_access_token().await?;
        let (method, url, metadata) = if let Some(file) = existing {
            (
                Method::PATCH,
                format!("{DRIVE_UPLOAD_API}/files/{}", file.id),
                serde_json::json!({ "name": name }),
            )
        } else {
            (
                Method::POST,
                format!("{DRIVE_UPLOAD_API}/files"),
                serde_json::json!({ "name": name, "parents": [parent.id] }),
            )
        };
        let response = self
            .http
            .request(method, url)
            .bearer_auth(&self.authorization.access_token)
            .query(&[("uploadType", "resumable"), ("supportsAllDrives", "true")])
            .header("X-Upload-Content-Type", "application/octet-stream")
            .header("X-Upload-Content-Length", total)
            .json(&metadata)
            .send()
            .await?;
        let response = require_success(response, "start a Google Drive resumable upload").await?;
        let upload_url = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| anyhow!("Google Drive did not return a resumable upload URL."))?
            .to_string();
        let file = tokio::fs::File::open(&source).await?;
        let response = self
            .http
            .put(upload_url)
            .header(header::CONTENT_LENGTH, total)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
            .send()
            .await?;
        require_success(response, "upload a Google Drive file").await?;
        Ok(total)
    }

    pub async fn download_file(&mut self, remote_path: &str, local_path: &str) -> Result<u64> {
        let file = self.resolve_path(remote_path).await?;
        if file.mime_type == "application/vnd.google-apps.folder" {
            bail!("A Google Drive folder cannot be downloaded as a file.");
        }
        self.ensure_access_token().await?;
        let response = if let Some((mime_type, _)) = export_format(&file.mime_type) {
            self.http
                .get(format!("{DRIVE_API}/files/{}/export", file.id))
                .bearer_auth(&self.authorization.access_token)
                .query(&[("mimeType", mime_type)])
                .send()
                .await?
        } else if file.mime_type.starts_with("application/vnd.google-apps.") {
            bail!("This Google-native item does not have a supported export format.");
        } else {
            self.http
                .get(format!("{DRIVE_API}/files/{}", file.id))
                .bearer_auth(&self.authorization.access_token)
                .query(&[("alt", "media"), ("supportsAllDrives", "true")])
                .send()
                .await?
        };
        let response = require_success(response, "download a Google Drive file").await?;
        let target = PathBuf::from(local_path);
        let temporary = temporary_download_path(&target)?;
        let result: Result<u64> = async {
            let mut output = tokio::fs::File::create(&temporary).await?;
            let mut stream = response.bytes_stream();
            let mut transferred = 0u64;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                output.write_all(&chunk).await?;
                transferred += chunk.len() as u64;
            }
            output.flush().await?;
            drop(output);
            tokio::fs::rename(&temporary, &target).await?;
            Ok(transferred)
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
        }
        result
    }

    pub async fn create_dir(&mut self, path: &str) -> Result<()> {
        let (parent_path, name) = split_parent_and_name(path)?;
        let parent = self.resolve_path(&parent_path).await?;
        if self.find_child(&parent.id, &name).await?.is_some() {
            return Ok(());
        }
        self.ensure_access_token().await?;
        let response = self
            .http
            .post(format!("{DRIVE_API}/files"))
            .bearer_auth(&self.authorization.access_token)
            .query(&[("supportsAllDrives", "true")])
            .json(&serde_json::json!({
                "name": name,
                "mimeType": "application/vnd.google-apps.folder",
                "parents": [parent.id]
            }))
            .send()
            .await?;
        require_success(response, "create a Google Drive folder").await?;
        Ok(())
    }

    pub async fn rename(&mut self, old_path: &str, new_path: &str) -> Result<()> {
        let source = self.resolve_path(old_path).await?;
        let (old_parent_path, _) = split_parent_and_name(old_path)?;
        let old_parent = self.resolve_path(&old_parent_path).await?;
        let (new_parent_path, new_name) = split_parent_and_name(new_path)?;
        let new_parent = self.resolve_path(&new_parent_path).await?;
        if self.find_child(&new_parent.id, &new_name).await?.is_some() {
            bail!("The Google Drive destination already exists.");
        }
        self.ensure_access_token().await?;
        let mut request = self
            .http
            .patch(format!("{DRIVE_API}/files/{}", source.id))
            .bearer_auth(&self.authorization.access_token)
            .query(&[("supportsAllDrives", "true")])
            .json(&serde_json::json!({ "name": new_name }));
        if old_parent.id != new_parent.id {
            request = request
                .query(&[("addParents", new_parent.id.as_str()), ("removeParents", old_parent.id.as_str())]);
        }
        let response = request.send().await?;
        require_success(response, "rename or move a Google Drive item").await?;
        Ok(())
    }

    pub async fn delete(&mut self, path: &str) -> Result<()> {
        if normalize_path(path)? == "/" {
            bail!("The Google Drive root cannot be deleted.");
        }
        let file = self.resolve_path(path).await?;
        self.ensure_access_token().await?;
        let response = self
            .http
            .patch(format!("{DRIVE_API}/files/{}", file.id))
            .bearer_auth(&self.authorization.access_token)
            .query(&[("supportsAllDrives", "true")])
            .json(&serde_json::json!({ "trashed": true }))
            .send()
            .await?;
        require_success(response, "move a Google Drive item to trash").await?;
        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        Ok(())
    }

    async fn resolve_path(&mut self, path: &str) -> Result<DriveFile> {
        let normalized = normalize_path(path)?;
        if let Some(file_id) =
            normalized.rsplit('/').find(|segment| !segment.is_empty()).and_then(drive_file_id_from_component)
        {
            if let Some(file) = self.file_cache.get(file_id) {
                return Ok(file.clone());
            }
            return self.get_file(file_id).await;
        }
        let mut current = DriveFile {
            id: "root".to_string(),
            name: "/".to_string(),
            mime_type: "application/vnd.google-apps.folder".to_string(),
            size: None,
            modified_time: None,
        };
        for segment in normalized.split('/').filter(|value| !value.is_empty()) {
            current = if let Some(file_id) = drive_file_id_from_component(segment) {
                self.get_file(file_id).await?
            } else {
                self.find_child(&current.id, segment)
                    .await?
                    .ok_or_else(|| anyhow!("Google Drive path does not exist: {normalized}"))?
            };
        }
        Ok(current)
    }

    async fn get_file(&mut self, file_id: &str) -> Result<DriveFile> {
        if file_id.is_empty()
            || !file_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        {
            bail!("Invalid Google Drive file ID in path.");
        }
        self.ensure_access_token().await?;
        let response = self
            .http
            .get(format!("{DRIVE_API}/files/{file_id}"))
            .bearer_auth(&self.authorization.access_token)
            .query(&[("fields", "id,name,mimeType,size,modifiedTime"), ("supportsAllDrives", "true")])
            .send()
            .await?;
        let response = require_success(response, "resolve a Google Drive file ID").await?;
        let file: DriveFile = response.json().await?;
        self.file_cache.insert(file.id.clone(), file.clone());
        Ok(file)
    }

    async fn find_child(&mut self, parent_id: &str, name: &str) -> Result<Option<DriveFile>> {
        self.ensure_access_token().await?;
        let escaped_name = name.replace('\\', "\\\\").replace('\'', "\\'");
        let query = format!("'{parent_id}' in parents and name = '{escaped_name}' and trashed = false");
        let response = self
            .http
            .get(format!("{DRIVE_API}/files"))
            .bearer_auth(&self.authorization.access_token)
            .query(&[
                ("q", query.as_str()),
                ("spaces", "drive"),
                ("pageSize", "2"),
                ("fields", "files(id,name,mimeType,size,modifiedTime)"),
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
            ])
            .send()
            .await?;
        let response = require_success(response, "resolve a Google Drive path").await?;
        let mut files = response.json::<DriveFileList>().await?.files;
        if files.len() > 1 {
            bail!("Google Drive contains multiple items named '{name}' in the same folder. Rename one in Google Drive before using this path.");
        }
        Ok(files.pop())
    }

    async fn ensure_access_token(&mut self) -> Result<()> {
        if self.authorization.expires_at > unix_time().saturating_add(60) {
            return Ok(());
        }
        let credentials = load_client_credentials_for(&self.authorization.client_id)?;
        let mut fields = vec![
            ("client_id", self.authorization.client_id.as_str()),
            ("refresh_token", self.authorization.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];
        if let Some(credentials) = credentials.as_ref() {
            fields.push(("client_secret", credentials.client_secret.as_str()));
        }
        let response = self.http.post(TOKEN_ENDPOINT).form(&fields).send().await?;
        let response = require_success(response, "refresh Google authorization").await?;
        let token: TokenResponse = response.json().await?;
        self.authorization.access_token = token.access_token;
        self.authorization.expires_at = unix_time().saturating_add(token.expires_in.unwrap_or(3600));
        save_authorization(&self.authorization)?;
        Ok(())
    }
}

pub async fn authorize(client_id: String) -> Result<GoogleAuthorizationStatus> {
    validate_client_id(&client_id)?;
    let client_id = client_id.trim().to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let verifier = random_token(96);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_token(48);
    let mut authorization_url = Url::parse(AUTHORIZATION_ENDPOINT)?;
    authorization_url
        .query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &format!("openid email {DRIVE_SCOPE}"))
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
    open_system_browser(authorization_url.as_str())?;

    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(300), listener.accept())
        .await
        .map_err(|_| anyhow!("Google authorization timed out."))??;
    let mut request = vec![0u8; 16 * 1024];
    let count = tokio::time::timeout(Duration::from_secs(10), socket.read(&mut request))
        .await
        .map_err(|_| anyhow!("Google authorization callback timed out."))??;
    let request = String::from_utf8_lossy(&request[..count]);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| anyhow!("Invalid Google authorization callback."))?;
    let callback = Url::parse(&format!("http://127.0.0.1:{port}{target}"))?;
    let parameters = callback.query_pairs().collect::<std::collections::HashMap<_, _>>();
    let response_html = if parameters.contains_key("error") {
        "<h1>Harbor Transfer</h1><p>Google authorization was cancelled. You may close this window.</p>"
    } else {
        "<h1>Harbor Transfer</h1><p>The authorization response was received. Return to Harbor Transfer while it completes the secure token exchange.</p>"
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_html.len(), response_html
    );
    socket.write_all(response.as_bytes()).await?;
    socket.shutdown().await?;

    if let Some(error) = parameters.get("error") {
        bail!("Google authorization was not completed: {error}");
    }
    if parameters.get("state").map(|value| value.as_ref()) != Some(state.as_str()) {
        bail!("Google authorization state verification failed.");
    }
    let code =
        parameters.get("code").ok_or_else(|| anyhow!("Google did not return an authorization code."))?;
    let http = Client::builder().https_only(true).timeout(Duration::from_secs(60)).build()?;
    let credentials = load_client_credentials_for(&client_id)?;
    let mut fields = vec![
        ("client_id", client_id.as_str()),
        ("code", code.as_ref()),
        ("code_verifier", verifier.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri.as_str()),
    ];
    if let Some(credentials) = credentials.as_ref() {
        fields.push(("client_secret", credentials.client_secret.as_str()));
    }
    let response = http.post(TOKEN_ENDPOINT).form(&fields).send().await?;
    let response = require_success(response, "exchange the Google authorization code").await?;
    let token: TokenResponse = response.json().await?;
    let refresh_token = token
        .refresh_token
        .ok_or_else(|| anyhow!("Google did not return a Refresh Token. Remove Harbor Transfer access from your Google Account and authorize again."))?;
    let user_info_response = http.get(USERINFO_ENDPOINT).bearer_auth(&token.access_token).send().await?;
    let user_info_response =
        require_success(user_info_response, "read the authorized Google account").await?;
    let user_info: UserInfo = user_info_response.json().await?;
    let email = user_info.email.unwrap_or_else(|| "Google Account".to_string());
    save_authorization(&StoredGoogleAuthorization {
        client_id,
        access_token: token.access_token,
        refresh_token,
        expires_at: unix_time().saturating_add(token.expires_in.unwrap_or(3600)),
        email: email.clone(),
    })?;
    Ok(GoogleAuthorizationStatus {
        authorized: true,
        email: Some(email),
        client_matches: true,
        credentials_ready: credentials.is_some(),
    })
}

pub fn authorization_status(client_id: &str) -> Result<GoogleAuthorizationStatus> {
    let saved = load_authorization()?;
    let credentials_ready = load_client_credentials_for(client_id)?.is_some();
    Ok(match saved {
        Some(value) => GoogleAuthorizationStatus {
            authorized: true,
            email: Some(value.email),
            client_matches: !client_id.trim().is_empty() && value.client_id == client_id.trim(),
            credentials_ready,
        },
        None => GoogleAuthorizationStatus {
            authorized: false,
            email: None,
            client_matches: false,
            credentials_ready,
        },
    })
}

pub fn import_client_credentials(path: &str) -> Result<String> {
    let raw = std::fs::read_to_string(path).context("Could not read the Google credentials JSON")?;
    let file: GoogleCredentialsFile =
        serde_json::from_str(&raw).context("The selected file is not valid Google credentials JSON")?;
    let installed = file.installed.ok_or_else(|| {
        anyhow!(
            "The selected OAuth credentials are not for a Desktop app (the JSON has no 'installed' section)."
        )
    })?;
    validate_client_id(&installed.client_id)?;
    if installed.client_secret.trim().is_empty() {
        bail!("The Google Desktop credentials JSON does not contain a client_secret.");
    }
    let credentials = StoredGoogleClientCredentials {
        client_id: installed.client_id.trim().to_string(),
        client_secret: installed.client_secret.trim().to_string(),
    };
    secret_store::store(GOOGLE_CREDENTIALS_VAULT_KEY, &serde_json::to_string(&credentials)?)?;
    let mut cache = keychain_cache().lock().map_err(|_| anyhow!("Google Keychain cache is unavailable."))?;
    cache.credentials_loaded = true;
    cache.credentials = Some(credentials.clone());
    Ok(credentials.client_id)
}

pub fn disconnect_authorization() -> Result<()> {
    secret_store::remove(GOOGLE_AUTHORIZATION_VAULT_KEY)?;
    let mut cache = keychain_cache().lock().map_err(|_| anyhow!("Google Keychain cache is unavailable."))?;
    cache.authorization_loaded = true;
    cache.authorization = None;
    Ok(())
}

pub fn open_setup_page(page: &str) -> Result<()> {
    let url = match page {
        "developers" => "https://developers.google.com/",
        "project" => "https://console.cloud.google.com/projectcreate",
        "drive-api" => "https://console.cloud.google.com/apis/library/drive.googleapis.com",
        "auth" => "https://console.cloud.google.com/auth/overview",
        "clients" => "https://console.cloud.google.com/auth/clients",
        _ => bail!("Unknown Google setup page."),
    };
    open_system_browser(url)
}

fn validate_client_id(client_id: &str) -> Result<()> {
    let value = client_id.trim();
    if value.len() < 20 || value.len() > 512 || !value.ends_with(".apps.googleusercontent.com") {
        bail!("Enter a valid Google OAuth Desktop Client ID ending in .apps.googleusercontent.com.");
    }
    Ok(())
}

fn authorization_entry() -> Result<keyring::Entry> {
    keyring::Entry::new("Harbor Transfer", GOOGLE_KEYCHAIN_ACCOUNT).map_err(Into::into)
}

fn credentials_entry() -> Result<keyring::Entry> {
    keyring::Entry::new("Harbor Transfer", GOOGLE_CREDENTIALS_KEYCHAIN_ACCOUNT).map_err(Into::into)
}

fn load_client_credentials_for(client_id: &str) -> Result<Option<StoredGoogleClientCredentials>> {
    if client_id.trim().is_empty() {
        return Ok(None);
    }
    let mut cache = keychain_cache().lock().map_err(|_| anyhow!("Google Keychain cache is unavailable."))?;
    if !cache.credentials_loaded {
        let value = match secret_store::lookup(GOOGLE_CREDENTIALS_VAULT_KEY)? {
            SecretLookup::Value(value) => Some(value),
            SecretLookup::Removed => None,
            SecretLookup::Missing => match credentials_entry()?.get_password() {
                Ok(value) => {
                    if let Err(error) = secret_store::store(GOOGLE_CREDENTIALS_VAULT_KEY, &value) {
                        tracing::warn!(
                            "Could not migrate legacy Google client credentials into the Keychain vault: {error}"
                        );
                    }
                    Some(value)
                }
                Err(keyring::Error::NoEntry) => None,
                Err(error) => return Err(error.into()),
            },
        };
        cache.credentials = value
            .map(|value| {
                serde_json::from_str::<StoredGoogleClientCredentials>(&value)
                    .context("Invalid Google client credentials in Keychain")
            })
            .transpose()?;
        cache.credentials_loaded = true;
    }
    Ok(cache.credentials.clone().filter(|credentials| credentials.client_id == client_id.trim()))
}

fn load_authorization() -> Result<Option<StoredGoogleAuthorization>> {
    let mut cache = keychain_cache().lock().map_err(|_| anyhow!("Google Keychain cache is unavailable."))?;
    if !cache.authorization_loaded {
        let value = match secret_store::lookup(GOOGLE_AUTHORIZATION_VAULT_KEY)? {
            SecretLookup::Value(value) => Some(value),
            SecretLookup::Removed => None,
            SecretLookup::Missing => match authorization_entry()?.get_password() {
                Ok(value) => {
                    if let Err(error) = secret_store::store(GOOGLE_AUTHORIZATION_VAULT_KEY, &value) {
                        tracing::warn!(
                            "Could not migrate legacy Google authorization into the Keychain vault: {error}"
                        );
                    }
                    Some(value)
                }
                Err(keyring::Error::NoEntry) => None,
                Err(error) => return Err(error.into()),
            },
        };
        cache.authorization = value
            .map(|value| serde_json::from_str(&value).context("Invalid Google authorization in Keychain"))
            .transpose()?;
        cache.authorization_loaded = true;
    }
    Ok(cache.authorization.clone())
}

fn save_authorization(authorization: &StoredGoogleAuthorization) -> Result<()> {
    secret_store::store(GOOGLE_AUTHORIZATION_VAULT_KEY, &serde_json::to_string(authorization)?)?;
    let mut cache = keychain_cache().lock().map_err(|_| anyhow!("Google Keychain cache is unavailable."))?;
    cache.authorization_loaded = true;
    cache.authorization = Some(authorization.clone());
    Ok(())
}

async fn require_success(response: reqwest::Response, action: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<GoogleErrorResponse>(&body)
        .ok()
        .and_then(|value| value.error_description.or_else(|| value.error.and_then(GoogleErrorValue::message)))
        .unwrap_or_else(|| body.chars().take(500).collect());
    if status == StatusCode::UNAUTHORIZED {
        bail!("Google authorization expired or was revoked. Authorize Google Drive again in Preferences.");
    }
    bail!("Failed to {action} ({status}): {detail}")
}

fn normalize_path(path: &str) -> Result<String> {
    if !path.starts_with('/') {
        bail!("Google Drive paths must be absolute.");
    }
    let mut segments = Vec::new();
    for segment in path.split('/').filter(|value| !value.is_empty()) {
        if segment == "." || segment == ".." {
            bail!("Google Drive paths cannot contain '.' or '..'.");
        }
        segments.push(segment);
    }
    Ok(if segments.is_empty() { "/".to_string() } else { format!("/{}", segments.join("/")) })
}

fn encode_drive_path_component(name: &str, file_id: &str) -> String {
    format!("{DRIVE_PATH_PREFIX}{}~{file_id}", URL_SAFE_NO_PAD.encode(name.as_bytes()))
}

fn drive_file_id_from_component(component: &str) -> Option<&str> {
    if let Some(encoded) = component.strip_prefix(DRIVE_PATH_PREFIX) {
        return encoded.rsplit_once('~').map(|(_, file_id)| file_id).filter(|value| !value.is_empty());
    }
    // Accept paths created by the first Google Drive preview build so an open
    // window can finish its current navigation after an in-place update.
    component.rsplit_once('\u{1f}').map(|(_, file_id)| file_id).filter(|value| !value.is_empty())
}

fn export_format(mime_type: &str) -> Option<(&'static str, &'static str)> {
    match mime_type {
        "application/vnd.google-apps.document" => {
            Some(("application/vnd.openxmlformats-officedocument.wordprocessingml.document", "docx"))
        }
        "application/vnd.google-apps.spreadsheet" => {
            Some(("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "xlsx"))
        }
        "application/vnd.google-apps.presentation" => {
            Some(("application/vnd.openxmlformats-officedocument.presentationml.presentation", "pptx"))
        }
        "application/vnd.google-apps.drawing" => Some(("application/pdf", "pdf")),
        "application/vnd.google-apps.script" => Some(("application/vnd.google-apps.script+json", "json")),
        _ => None,
    }
}

fn split_parent_and_name(path: &str) -> Result<(String, String)> {
    let normalized = normalize_path(path)?;
    if normalized == "/" {
        bail!("The Google Drive root cannot be modified.");
    }
    let separator = normalized.rfind('/').ok_or_else(|| anyhow!("Invalid Google Drive path."))?;
    let name = normalized[separator + 1..].to_string();
    let parent = if separator == 0 { "/".to_string() } else { normalized[..separator].to_string() };
    Ok((parent, name))
}

fn temporary_download_path(target: &Path) -> Result<PathBuf> {
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("Invalid download destination."))?;
    Ok(target.with_file_name(format!(".{name}.harbor-download")))
}

fn random_token(length: usize) -> String {
    rand::thread_rng().sample_iter(&Alphanumeric).take(length).map(char::from).collect()
}

fn unix_time() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn open_system_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(url).status()?;
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd").args(["/C", "start", "", url]).status()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = std::process::Command::new("xdg-open").arg(url).status()?;
    if !status.success() {
        bail!("Could not open the system browser for Google authorization.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        drive_file_id_from_component, encode_drive_path_component, export_format, normalize_path,
        split_parent_and_name, validate_client_id,
    };

    #[test]
    fn validates_google_paths() {
        assert_eq!(normalize_path("/").unwrap(), "/");
        assert_eq!(normalize_path("/Folder/File.txt").unwrap(), "/Folder/File.txt");
        assert!(normalize_path("relative").is_err());
        assert!(normalize_path("/Folder/../Secret").is_err());
        assert_eq!(split_parent_and_name("/Folder/File.txt").unwrap(), ("/Folder".into(), "File.txt".into()));
    }

    #[test]
    fn validates_desktop_client_id_shape() {
        assert!(validate_client_id("123456789-example.apps.googleusercontent.com").is_ok());
        assert!(validate_client_id("not-a-client-id").is_err());
    }

    #[test]
    fn drive_components_keep_slashes_out_and_preserve_the_file_id() {
        let component = encode_drive_path_component("doc/Objective.md 日本語", "file_ID-123");
        assert!(!component.contains('/'));
        assert_eq!(drive_file_id_from_component(&component), Some("file_ID-123"));
    }

    #[test]
    fn chooses_editable_exports_for_google_workspace_documents() {
        assert_eq!(
            export_format("application/vnd.google-apps.document"),
            Some(("application/vnd.openxmlformats-officedocument.wordprocessingml.document", "docx"))
        );
        assert_eq!(
            export_format("application/vnd.google-apps.spreadsheet").map(|value| value.1),
            Some("xlsx")
        );
        assert!(export_format("application/pdf").is_none());
    }
}

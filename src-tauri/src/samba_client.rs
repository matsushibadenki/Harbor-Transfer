use crate::sftp_client::{FileEntry, FileEntryType};
use anyhow::{Context, Result};
use smb2::client::connection::Connection;
use smb2::client::session::Session;
use smb2::client::{ClientConfig, SmbClient, Tree};
use smb2::msg::close::CloseRequest;
use smb2::msg::create::{CreateDisposition, CreateRequest, CreateResponse, ImpersonationLevel, ShareAccess};
use smb2::msg::set_info::{InfoType, SetInfoRequest};
use smb2::pack::{FileTime, ReadCursor, Unpack};
use smb2::types::flags::FileAccessMask;
use smb2::types::{Command, OplockLevel};
use std::collections::HashMap;
use std::future::Future;
use std::time::{Duration, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const LOCAL_READ_CHUNK: usize = 1024 * 1024;

/// Connection settings for one SMB share. Passwords are supplied from
/// Keychain by the command layer and are never serialized with bookmarks.
#[derive(Clone)]
pub struct SambaConfig {
    pub host: String,
    pub port: u16,
    pub share: String,
    pub username: String,
    pub password: String,
    pub domain: String,
    pub guest: bool,
    pub probe_path: String,
}

pub struct SambaClient {
    client: Option<SmbClient>,
    tree: Option<Tree>,
    config: SambaConfig,
}

impl SambaClient {
    pub async fn connect(mut config: SambaConfig) -> Result<Self> {
        let host = config.host.trim();
        anyhow::ensure!(!host.is_empty(), "An SMB server is required.");
        let share_name = validate_share_name(&config.share)?;
        config.share = share_name.clone();
        let address = smb_server_address(host, config.port);
        let (username, password, domain) = if config.guest {
            (String::new(), String::new(), String::new())
        } else {
            anyhow::ensure!(!config.username.trim().is_empty(), "An SMB username is required.");
            (config.username.clone(), config.password.clone(), config.domain.clone())
        };
        let mut client = SmbClient::connect(ClientConfig {
            addr: address.clone(),
            timeout: Duration::from_secs(10),
            username,
            password,
            domain,
            auto_reconnect: true,
            compression: true,
            dfs_enabled: true,
            dfs_target_overrides: HashMap::new(),
        })
        .await
        .with_context(|| format!("Failed to connect to SMB server {address}"))?;
        let mut tree = client
            .connect_share(&share_name)
            .await
            .with_context(|| format!("Failed to open SMB share '{share_name}'"))?;
        let probe_path = smb_relative_path(&config.probe_path)?;
        client
            .list_directory(&mut tree, &probe_path)
            .await
            .with_context(|| format!("Failed to open SMB directory '{}'.", config.probe_path))?;
        Ok(Self { client: Some(client), tree: Some(tree), config })
    }

    fn parts_mut(&mut self) -> Result<(&mut SmbClient, &mut Tree)> {
        let client = self.client.as_mut().context("SMB client is disconnected.")?;
        let tree = self.tree.as_mut().context("SMB share is disconnected.")?;
        Ok((client, tree))
    }

    pub async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>> {
        let relative = smb_relative_path(path)?;
        let (client, tree) = self.parts_mut()?;
        let mut entries = client
            .list_directory(tree, &relative)
            .await
            .with_context(|| format!("Failed to list SMB directory '{path}'"))?
            .into_iter()
            .filter(|entry| entry.name != "." && entry.name != "..")
            .map(|entry| FileEntry {
                name: entry.name,
                path_component: None,
                download_name: None,
                size: if entry.is_directory { 0 } else { entry.size },
                modified: entry
                    .modified
                    .to_system_time()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| format_unix_timestamp(duration.as_secs())),
                permissions: None,
                file_type: if entry.is_directory { FileEntryType::Directory } else { FileEntryType::File },
                owner: None,
                group: None,
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            let left_directory = matches!(left.file_type, FileEntryType::Directory);
            let right_directory = matches!(right.file_type, FileEntryType::Directory);
            right_directory
                .cmp(&left_directory)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> Result<u64> {
        self.upload_file_with_progress(local_path, remote_path, |_, _| async { Ok(()) }).await
    }

    pub async fn upload_file_with_progress<F, Fut>(
        &mut self,
        local_path: &str,
        remote_path: &str,
        mut on_progress: F,
    ) -> Result<u64>
    where
        F: FnMut(u64, u64) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let relative = smb_relative_path(remote_path)?;
        anyhow::ensure!(!relative.is_empty(), "The SMB share root cannot be overwritten.");
        let mut source = tokio::fs::File::open(local_path)
            .await
            .with_context(|| format!("Failed to open local file '{local_path}'"))?;
        let total = source.metadata().await?.len();
        let (client, tree) = self.parts_mut()?;
        let mut writer = client
            .create_file_writer(tree, &relative)
            .await
            .with_context(|| format!("Failed to open SMB upload '{remote_path}'"))?;
        let mut buffer = vec![0_u8; LOCAL_READ_CHUNK];
        let mut sent = 0_u64;
        loop {
            let read = source.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            writer.write_chunk(&buffer[..read]).await?;
            sent += read as u64;
            if let Err(error) = on_progress(sent, total).await {
                let _ = writer.abort().await;
                let _ = client.delete_file(tree, &relative).await;
                return Err(error);
            }
        }
        writer.finish().await.with_context(|| format!("Failed to upload '{remote_path}' to SMB"))
    }

    pub async fn download_file(&mut self, remote_path: &str, local_path: &str) -> Result<u64> {
        self.download_file_with_progress(remote_path, local_path, |_, _| async { Ok(()) }).await
    }

    pub async fn download_file_with_progress<F, Fut>(
        &mut self,
        remote_path: &str,
        local_path: &str,
        mut on_progress: F,
    ) -> Result<u64>
    where
        F: FnMut(u64, u64) -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        let relative = smb_relative_path(remote_path)?;
        anyhow::ensure!(!relative.is_empty(), "The SMB share root cannot be downloaded as a file.");
        let (client, tree) = self.parts_mut()?;
        let mut download = client
            .download(tree, &relative)
            .await
            .with_context(|| format!("Failed to open SMB file '{remote_path}'"))?;
        let total = download.size();
        let mut destination = tokio::fs::File::create(local_path)
            .await
            .with_context(|| format!("Failed to create local file '{local_path}'"))?;
        let mut written = 0_u64;
        while let Some(chunk) = download.next_chunk().await {
            let chunk =
                chunk.with_context(|| format!("Failed while downloading SMB file '{remote_path}'"))?;
            destination.write_all(&chunk).await?;
            written += chunk.len() as u64;
            if let Err(error) = on_progress(written, total).await {
                drop(destination);
                let _ = tokio::fs::remove_file(local_path).await;
                return Err(error);
            }
        }
        destination.flush().await?;
        Ok(written)
    }

    pub async fn create_dir(&mut self, path: &str) -> Result<()> {
        let relative = smb_relative_path(path)?;
        anyhow::ensure!(!relative.is_empty(), "The SMB share root already exists.");
        let (client, tree) = self.parts_mut()?;
        client.create_directory(tree, &relative).await.map_err(Into::into)
    }

    pub async fn rename(&mut self, old_path: &str, new_path: &str) -> Result<()> {
        let old_relative = smb_relative_path(old_path)?;
        let new_relative = smb_relative_path(new_path)?;
        anyhow::ensure!(
            !old_relative.is_empty() && !new_relative.is_empty(),
            "The SMB share root cannot be renamed."
        );
        let (client, tree) = self.parts_mut()?;
        client.rename(tree, &old_relative, &new_relative).await.map_err(Into::into)
    }

    pub async fn set_modified_time(&mut self, path: &str, unix_seconds: u32) -> Result<()> {
        let relative = smb_relative_path(path)?;
        anyhow::ensure!(!relative.is_empty(), "The SMB share root metadata cannot be changed.");
        let is_directory = {
            let (client, tree) = self.parts_mut()?;
            client
                .stat(tree, &relative)
                .await
                .with_context(|| format!("Failed to inspect SMB item '{path}'"))?
                .is_directory
        };
        set_modified_time_with_new_session(&self.config, &relative, unix_seconds, is_directory)
            .await
            .with_context(|| format!("Failed to change the modified date for SMB item '{path}'"))
    }

    pub async fn delete_file(&mut self, path: &str) -> Result<()> {
        let relative = smb_relative_path(path)?;
        anyhow::ensure!(!relative.is_empty(), "The SMB share root cannot be deleted.");
        let (client, tree) = self.parts_mut()?;
        client.delete_file(tree, &relative).await.map_err(Into::into)
    }

    pub async fn delete_dir(&mut self, path: &str) -> Result<()> {
        let relative = smb_relative_path(path)?;
        anyhow::ensure!(!relative.is_empty(), "The SMB share root cannot be deleted.");
        let (client, tree) = self.parts_mut()?;
        client.delete_directory(tree, &relative).await.map_err(Into::into)
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        if let (Some(client), Some(tree)) = (self.client.as_mut(), self.tree.take()) {
            client.disconnect_share(&tree).await?;
        }
        self.client = None;
        self.config.password.clear();
        Ok(())
    }
}

async fn set_modified_time_with_new_session(
    config: &SambaConfig,
    relative_path: &str,
    unix_seconds: u32,
    is_directory: bool,
) -> Result<()> {
    let address = smb_server_address(config.host.trim(), config.port);
    let mut connection = Connection::connect(&address, Duration::from_secs(10)).await?;
    connection.negotiate().await?;
    let (username, password, domain) = if config.guest {
        ("", "", "")
    } else {
        (config.username.as_str(), config.password.as_str(), config.domain.as_str())
    };
    let _session = Session::setup(&mut connection, username, password, domain).await?;
    let tree = Tree::connect(&mut connection, &config.share).await?;
    let create = CreateRequest {
        requested_oplock_level: OplockLevel::None,
        impersonation_level: ImpersonationLevel::Impersonation,
        desired_access: FileAccessMask::new(
            FileAccessMask::FILE_WRITE_ATTRIBUTES | FileAccessMask::SYNCHRONIZE,
        ),
        file_attributes: 0,
        share_access: ShareAccess(
            ShareAccess::FILE_SHARE_READ | ShareAccess::FILE_SHARE_WRITE | ShareAccess::FILE_SHARE_DELETE,
        ),
        create_disposition: CreateDisposition::FileOpen,
        // MS-SMB2: FILE_DIRECTORY_FILE / FILE_NON_DIRECTORY_FILE.
        create_options: if is_directory { 0x0000_0001 } else { 0x0000_0040 },
        name: smb_wire_path(&tree, relative_path),
        create_contexts: Vec::new(),
    };
    let create_frame = connection.execute(Command::Create, &create, Some(tree.tree_id)).await?;
    if create_frame.header.status.is_error() {
        anyhow::bail!("SMB CREATE returned {:?}", create_frame.header.status);
    }
    let mut cursor = ReadCursor::new(&create_frame.body);
    let file_id = CreateResponse::unpack(&mut cursor)?.file_id;
    let modified = FileTime::from_system_time(UNIX_EPOCH + Duration::from_secs(unix_seconds as u64)).0;
    let mut basic_information = Vec::with_capacity(40);
    basic_information.extend_from_slice(&0_u64.to_le_bytes());
    basic_information.extend_from_slice(&0_u64.to_le_bytes());
    basic_information.extend_from_slice(&modified.to_le_bytes());
    basic_information.extend_from_slice(&0_u64.to_le_bytes());
    basic_information.extend_from_slice(&0_u32.to_le_bytes());
    basic_information.extend_from_slice(&0_u32.to_le_bytes());
    let set_info = SetInfoRequest {
        info_type: InfoType::File,
        file_info_class: 4,
        additional_information: 0,
        file_id,
        buffer: basic_information,
    };
    let set_result = connection.execute(Command::SetInfo, &set_info, Some(tree.tree_id)).await;
    let close = CloseRequest { flags: 0, file_id };
    let close_result = connection.execute(Command::Close, &close, Some(tree.tree_id)).await;
    let set_frame = set_result?;
    if set_frame.header.status.is_error() {
        anyhow::bail!("SMB SET_INFO returned {:?}", set_frame.header.status);
    }
    close_result?;
    tree.disconnect(&mut connection).await?;
    Ok(())
}

fn smb_wire_path(tree: &Tree, relative_path: &str) -> String {
    let encoded = smb2::encode_path(relative_path);
    if tree.is_dfs {
        let hostname = tree.server.split(':').next().unwrap_or(&tree.server);
        format!("{hostname}\\{}\\{encoded}", tree.share_name)
    } else {
        encoded
    }
}

fn validate_share_name(share: &str) -> Result<String> {
    let share = share.trim().trim_matches(['/', '\\']);
    anyhow::ensure!(!share.is_empty(), "An SMB share name is required.");
    anyhow::ensure!(
        !share.contains(['/', '\\', '\0']),
        "The SMB share name must not contain path separators."
    );
    Ok(share.to_string())
}

fn smb_server_address(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn smb_relative_path(path: &str) -> Result<String> {
    let mut components = Vec::new();
    let normalized = path.replace('\\', "/");
    for component in normalized.split('/') {
        match component {
            "" | "." => continue,
            ".." => anyhow::bail!("Parent-directory components are not allowed in SMB paths."),
            value if value.contains('\0') => anyhow::bail!("SMB paths must not contain NUL characters."),
            value => components.push(value),
        }
    }
    Ok(components.join("/"))
}

fn format_unix_timestamp(secs: u64) -> String {
    let days = secs / 86_400;
    let remaining = secs % 86_400;
    let (year, month, day) = days_to_ymd(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        remaining / 3_600,
        (remaining % 3_600) / 60,
        remaining % 60
    )
}

fn days_to_ymd(mut days: i64) -> (i64, u32, u32) {
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = (days - era * 146_097) as u32;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 { month_prime + 3 } else { month_prime - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::{smb_relative_path, smb_server_address, validate_share_name, SambaClient, SambaConfig};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    #[test]
    fn normalizes_ui_paths_to_share_relative_paths() {
        assert_eq!(smb_relative_path("/").unwrap(), "");
        assert_eq!(smb_relative_path("/資料/写真.jpg").unwrap(), "資料/写真.jpg");
        assert!(smb_relative_path("/safe/../secret").is_err());
    }

    #[test]
    fn validates_share_names_and_ipv6_addresses() {
        assert_eq!(validate_share_name("/Documents/").unwrap(), "Documents");
        assert!(validate_share_name("Documents/private").is_err());
        assert_eq!(smb_server_address("2001:db8::1", 445), "[2001:db8::1]:445");
    }

    fn live_config() -> Option<SambaConfig> {
        let host = std::env::var("SMB_TEST_HOST").ok()?;
        Some(SambaConfig {
            host,
            port: std::env::var("SMB_TEST_PORT").ok()?.parse().ok()?,
            share: std::env::var("SMB_TEST_SHARE").ok()?,
            username: std::env::var("SMB_TEST_USER").ok()?,
            password: std::env::var("SMB_TEST_PASS").ok()?,
            domain: String::new(),
            guest: false,
            probe_path: "/".to_string(),
        })
    }

    #[tokio::test]
    async fn live_samba_round_trip_unicode_and_reconnect() {
        let Some(config) = live_config() else { return };
        let local = tempfile::tempdir().expect("temporary directory");
        let source_path = local.path().join("source.bin");
        let downloaded_path = local.path().join("downloaded.bin");
        let payload = vec![0x5a_u8; 2 * 1024 * 1024 + 731];
        std::fs::write(&source_path, &payload).expect("write source");

        let mut client = SambaClient::connect(config).await.expect("connect Samba");
        client.create_dir("/空のフォルダ").await.expect("create Unicode directory");
        let upload_progress = Arc::new(AtomicU64::new(0));
        let upload_progress_callback = upload_progress.clone();
        client
            .upload_file_with_progress(
                source_path.to_str().unwrap(),
                "/空のフォルダ/転送.bin",
                move |done, _| {
                    let progress = upload_progress_callback.clone();
                    async move {
                        progress.store(done, Ordering::Release);
                        Ok(())
                    }
                },
            )
            .await
            .expect("upload file");
        assert_eq!(upload_progress.load(Ordering::Acquire), payload.len() as u64);
        let entries = client.list_dir("/空のフォルダ").await.expect("list directory");
        assert!(entries.iter().any(|entry| entry.name == "転送.bin" && entry.size == payload.len() as u64));
        client.rename("/空のフォルダ/転送.bin", "/空のフォルダ/renamed.bin").await.expect("rename file");
        let expected_modified = 1_700_000_000_u32;
        let expected_modified_text = super::format_unix_timestamp(expected_modified as u64);
        client
            .set_modified_time("/空のフォルダ/renamed.bin", expected_modified)
            .await
            .expect("change file modified date");
        client
            .set_modified_time("/空のフォルダ", expected_modified)
            .await
            .expect("change directory modified date");
        let entries = client.list_dir("/空のフォルダ").await.expect("list after metadata change");
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.name == "renamed.bin")
                .and_then(|entry| entry.modified.as_deref()),
            Some(expected_modified_text.as_str())
        );
        let root_entries = client.list_dir("/").await.expect("list root after metadata change");
        assert_eq!(
            root_entries
                .iter()
                .find(|entry| entry.name == "空のフォルダ")
                .and_then(|entry| entry.modified.as_deref()),
            Some(expected_modified_text.as_str())
        );
        let download_progress = Arc::new(AtomicU64::new(0));
        let download_progress_callback = download_progress.clone();
        client
            .download_file_with_progress(
                "/空のフォルダ/renamed.bin",
                downloaded_path.to_str().unwrap(),
                move |done, _| {
                    let progress = download_progress_callback.clone();
                    async move {
                        progress.store(done, Ordering::Release);
                        Ok(())
                    }
                },
            )
            .await
            .expect("download file");
        assert_eq!(download_progress.load(Ordering::Acquire), payload.len() as u64);
        assert_eq!(std::fs::read(&downloaded_path).expect("read download"), payload);
        let cancelled = client
            .upload_file_with_progress(
                source_path.to_str().unwrap(),
                "/空のフォルダ/cancelled.bin",
                |_, _| async { anyhow::bail!("cancelled by integration test") },
            )
            .await;
        assert!(cancelled.is_err());
        let entries = client.list_dir("/空のフォルダ").await.expect("list after cancellation");
        assert!(!entries.iter().any(|entry| entry.name == "cancelled.bin"));
        client.delete_file("/空のフォルダ/renamed.bin").await.expect("delete file");
        client.delete_dir("/空のフォルダ").await.expect("delete directory");
        client.disconnect().await.expect("disconnect");

        let reconnect = live_config().expect("live configuration");
        let mut client = SambaClient::connect(reconnect).await.expect("reconnect Samba");
        client.list_dir("/").await.expect("list after reconnect");
        client.disconnect().await.expect("disconnect after reconnect");

        let mut read_only = live_config().expect("read-only configuration");
        read_only.share = "ReadOnly".to_string();
        let mut client = SambaClient::connect(read_only).await.expect("connect read-only share");
        assert!(client.create_dir("/must-be-rejected").await.is_err());
        client.disconnect().await.expect("disconnect read-only share");
    }
}

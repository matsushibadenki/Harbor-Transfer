use crate::sftp_client::{FileEntry, FileEntryType};
use anyhow::{anyhow, Result};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::{ByteStream, Length};
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_LIST_ENTRIES: usize = 10_000;
const MIN_MULTIPART_PART_SIZE: u64 = 8 * 1024 * 1024;
const MAX_MULTIPART_PART_SIZE: u64 = 5 * 1024 * 1024 * 1024;
const MAX_MULTIPART_PARTS: u64 = 10_000;
const UNSUPPORTED_MUTATION_ERROR: &str =
    "This S3 operation is disabled until copy/delete safety is implemented.";

// Do not derive `Debug`: this value contains credentials loaded from Keychain.
#[derive(Clone)]
pub struct S3Config {
    pub region: String,
    pub bucket: String,
    pub endpoint: Option<String>,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub force_path_style: bool,
    pub probe_path: String,
}

pub struct S3Client {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Client {
    pub async fn connect(config: &S3Config) -> Result<Self> {
        validate_config(config)?;
        let mut result = Self::from_config(config).await;
        result.list_dir(&config.probe_path).await?;
        Ok(result)
    }

    async fn from_config(config: &S3Config) -> Self {
        let credentials = Credentials::new(
            config.access_key_id.clone(),
            config.secret_access_key.clone(),
            config.session_token.clone(),
            None,
            "harbor-transfer-keychain",
        );
        let shared = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(config.region.clone()))
            .credentials_provider(credentials)
            .load()
            .await;
        let mut builder =
            aws_sdk_s3::config::Builder::from(&shared).force_path_style(config.force_path_style);
        if let Some(endpoint) = config.endpoint.as_ref() {
            builder = builder.endpoint_url(endpoint);
        }
        Self { client: aws_sdk_s3::Client::from_conf(builder.build()), bucket: config.bucket.clone() }
    }

    #[cfg(test)]
    async fn connect_insecure_local_for_test(config: &S3Config) -> Result<Self> {
        let endpoint = config.endpoint.as_ref().ok_or_else(|| anyhow!("S3 test endpoint is required."))?;
        let parsed = reqwest::Url::parse(endpoint).map_err(|_| anyhow!("Invalid S3 test endpoint."))?;
        let local_host = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
        if parsed.scheme() != "http" || !local_host {
            return Err(anyhow!("Insecure S3 tests are restricted to a loopback HTTP endpoint."));
        }
        if config.region.trim().is_empty()
            || config.bucket.trim().is_empty()
            || config.access_key_id.trim().is_empty()
            || config.secret_access_key.is_empty()
        {
            return Err(anyhow!("Incomplete S3 test configuration."));
        }
        Ok(Self::from_config(config).await)
    }

    pub async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>> {
        let prefix = path_to_prefix(path)?;
        let mut continuation_token = None;
        let mut entries = Vec::new();
        let mut names = HashSet::new();

        loop {
            let output = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&prefix)
                .delimiter("/")
                .set_continuation_token(continuation_token)
                .send()
                .await
                .map_err(|error| anyhow!("Failed to list S3 bucket '{}': {error}", self.bucket))?;

            for common in output.common_prefixes() {
                let Some(value) = common.prefix() else { continue };
                let name = value.strip_prefix(&prefix).unwrap_or(value).trim_end_matches('/');
                if name.is_empty() || !names.insert(name.to_string()) {
                    continue;
                }
                entries.push(FileEntry {
                    name: name.to_string(),
                    size: 0,
                    modified: None,
                    permissions: None,
                    file_type: FileEntryType::Directory,
                    owner: None,
                    group: None,
                });
            }

            for object in output.contents() {
                let Some(key) = object.key() else { continue };
                let name = key.strip_prefix(&prefix).unwrap_or(key);
                if name.is_empty()
                    || name.ends_with('/')
                    || name.contains('/')
                    || !names.insert(name.to_string())
                {
                    continue;
                }
                entries.push(FileEntry {
                    name: name.to_string(),
                    size: object.size().unwrap_or_default().max(0) as u64,
                    modified: object.last_modified().map(ToString::to_string),
                    permissions: None,
                    file_type: FileEntryType::File,
                    owner: None,
                    group: None,
                });
            }

            if entries.len() > MAX_LIST_ENTRIES {
                return Err(anyhow!(
                    "S3 listing exceeds {MAX_LIST_ENTRIES} entries. Choose a narrower prefix."
                ));
            }
            continuation_token = output.next_continuation_token().map(str::to_string);
            if continuation_token.is_none() {
                break;
            }
        }

        entries.sort_by(|left, right| {
            let left_kind = matches!(left.file_type, FileEntryType::File);
            let right_kind = matches!(right.file_type, FileEntryType::File);
            left_kind.cmp(&right_kind).then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
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
        let key = path_to_key(remote_path)?;
        let source = PathBuf::from(local_path);
        let total = tokio::fs::metadata(&source)
            .await
            .map_err(|error| anyhow!("Failed to inspect local file '{local_path}': {error}"))?
            .len();
        if total == 0 {
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(ByteStream::from_static(&[]))
                .send()
                .await
                .map_err(|error| anyhow!("Failed to upload empty S3 object '{key}': {error}"))?;
            on_progress(0, 0).await?;
            return Ok(0);
        }
        let part_size = multipart_part_size(total)?;

        let created = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|error| anyhow!("Failed to start multipart upload for '{key}': {error}"))?;
        let upload_id =
            created.upload_id().ok_or_else(|| anyhow!("S3 did not return a multipart upload ID."))?;
        let mut parts = Vec::new();
        let result: Result<u64> = async {
            let mut transferred = 0u64;
            let mut part_number = 1i32;
            while transferred < total {
                let part_length = part_size.min(total - transferred);
                let body = ByteStream::read_from()
                    .path(&source)
                    .offset(transferred)
                    .length(Length::Exact(part_length))
                    .buffer_size(64 * 1024)
                    .build()
                    .await
                    .map_err(|error| anyhow!("Failed to read multipart source '{local_path}': {error}"))?;
                let uploaded = self
                    .client
                    .upload_part()
                    .bucket(&self.bucket)
                    .key(&key)
                    .upload_id(upload_id)
                    .part_number(part_number)
                    .body(body)
                    .send()
                    .await
                    .map_err(|error| {
                        anyhow!("Failed to upload S3 part {part_number} for '{key}': {error}")
                    })?;
                let e_tag = uploaded
                    .e_tag()
                    .ok_or_else(|| anyhow!("S3 did not return an ETag for uploaded part {part_number}."))?;
                parts.push(CompletedPart::builder().part_number(part_number).e_tag(e_tag).build());
                transferred += part_length;
                on_progress(transferred, total).await?;
                part_number =
                    part_number.checked_add(1).ok_or_else(|| anyhow!("Too many multipart parts."))?;
            }
            self.client
                .complete_multipart_upload()
                .bucket(&self.bucket)
                .key(&key)
                .upload_id(upload_id)
                .multipart_upload(CompletedMultipartUpload::builder().set_parts(Some(parts)).build())
                .send()
                .await
                .map_err(|error| anyhow!("Failed to complete multipart upload for '{key}': {error}"))?;
            Ok(transferred)
        }
        .await;

        if result.is_err() {
            let _ = self
                .client
                .abort_multipart_upload()
                .bucket(&self.bucket)
                .key(&key)
                .upload_id(upload_id)
                .send()
                .await;
        }
        result
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
        let key = path_to_key(remote_path)?;
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|error| anyhow!("Failed to download S3 object '{key}': {error}"))?;
        let total = output.content_length().unwrap_or_default().max(0) as u64;
        let target = PathBuf::from(local_path);
        let temporary = temporary_download_path(&target)?;
        let result: Result<u64> = async {
            let mut body = output.body.into_async_read();
            let mut file = tokio::fs::File::create(&temporary).await?;
            let mut buffer = vec![0u8; 64 * 1024];
            let mut transferred = 0u64;
            loop {
                let count = body.read(&mut buffer).await?;
                if count == 0 {
                    break;
                }
                file.write_all(&buffer[..count]).await?;
                transferred += count as u64;
                on_progress(transferred, total).await?;
            }
            file.flush().await?;
            drop(file);
            tokio::fs::rename(&temporary, &target).await?;
            Ok(transferred)
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temporary).await;
        }
        result
    }

    pub async fn create_dir(&mut self, _path: &str) -> Result<()> {
        Err(anyhow!(UNSUPPORTED_MUTATION_ERROR))
    }

    pub async fn rename(&mut self, _old_path: &str, _new_path: &str) -> Result<()> {
        Err(anyhow!(UNSUPPORTED_MUTATION_ERROR))
    }

    pub async fn delete_file(&mut self, _path: &str) -> Result<()> {
        Err(anyhow!(UNSUPPORTED_MUTATION_ERROR))
    }

    pub async fn delete_dir(&mut self, _path: &str) -> Result<()> {
        Err(anyhow!(UNSUPPORTED_MUTATION_ERROR))
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        Ok(())
    }
}

fn validate_config(config: &S3Config) -> Result<()> {
    if config.region.trim().is_empty()
        || config.bucket.trim().is_empty()
        || config.access_key_id.trim().is_empty()
        || config.secret_access_key.is_empty()
    {
        return Err(anyhow!("S3 region, bucket, Access Key ID, and Secret Access Key are required."));
    }
    if let Some(endpoint) = config.endpoint.as_ref() {
        let parsed =
            reqwest::Url::parse(endpoint).map_err(|_| anyhow!("The S3 endpoint is not a valid URL."))?;
        if parsed.scheme() != "https" || parsed.host_str().is_none() {
            return Err(anyhow!("The S3 endpoint must be an HTTPS URL."));
        }
    }
    path_to_prefix(&config.probe_path)?;
    Ok(())
}

fn path_to_prefix(path: &str) -> Result<String> {
    if path.contains('\0') || path.split('/').any(|segment| segment == "..") {
        return Err(anyhow!("The S3 prefix contains an unsafe path segment."));
    }
    let trimmed = path.trim().trim_matches('/');
    Ok(if trimmed.is_empty() { String::new() } else { format!("{trimmed}/") })
}

fn path_to_key(path: &str) -> Result<String> {
    if path.contains('\0') || path.split('/').any(|segment| segment == "..") {
        return Err(anyhow!("The S3 object path contains an unsafe segment."));
    }
    let key = path.trim_start_matches('/');
    if key.is_empty() || key.ends_with('/') {
        return Err(anyhow!("An S3 object key is required."));
    }
    Ok(key.to_string())
}

fn temporary_download_path(target: &Path) -> Result<PathBuf> {
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("Invalid local file name."))?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    Ok(target.with_file_name(format!(".{name}.harbor-transfer-{}-{nonce}.part", std::process::id())))
}

fn multipart_part_size(total: u64) -> Result<u64> {
    let required = total.div_ceil(MAX_MULTIPART_PARTS);
    let mebibyte = 1024 * 1024;
    let aligned = required.div_ceil(mebibyte) * mebibyte;
    let part_size = aligned.max(MIN_MULTIPART_PART_SIZE);
    if part_size > MAX_MULTIPART_PART_SIZE {
        return Err(anyhow!("The file exceeds the S3 multipart object-size limit."));
    }
    Ok(part_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{stream, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    fn config() -> S3Config {
        S3Config {
            region: "ap-northeast-1".into(),
            bucket: "example-bucket".into(),
            endpoint: None,
            access_key_id: "test-access-key".into(),
            secret_access_key: "test-secret".into(),
            session_token: None,
            force_path_style: false,
            probe_path: "/".into(),
        }
    }

    #[test]
    fn converts_ui_paths_to_s3_prefixes() {
        assert_eq!(path_to_prefix("/").unwrap(), "");
        assert_eq!(path_to_prefix("/photos/2026").unwrap(), "photos/2026/");
        assert!(path_to_prefix("/photos/../private").is_err());
    }

    #[test]
    fn rejects_insecure_custom_endpoints() {
        let mut value = config();
        value.endpoint = Some("http://storage.example.com".into());
        assert!(validate_config(&value).is_err());
        value.endpoint = Some("https://storage.example.com".into());
        assert!(validate_config(&value).is_ok());
    }

    #[test]
    fn converts_ui_paths_to_object_keys_without_accepting_directories() {
        assert_eq!(path_to_key("/photos/海.jpg").unwrap(), "photos/海.jpg");
        assert!(path_to_key("/").is_err());
        assert!(path_to_key("/photos/").is_err());
        assert!(path_to_key("/photos/../secret.txt").is_err());
    }

    #[test]
    fn creates_partial_download_next_to_the_destination() {
        let target = PathBuf::from("/tmp/example.txt");
        let partial = temporary_download_path(&target).unwrap();
        assert_eq!(partial.parent(), target.parent());
        assert!(partial.file_name().unwrap().to_string_lossy().starts_with(".example.txt.harbor-transfer-"));
        assert!(partial.file_name().unwrap().to_string_lossy().ends_with(".part"));
    }

    #[test]
    fn chooses_a_valid_part_size_without_exceeding_ten_thousand_parts() {
        assert_eq!(multipart_part_size(1).unwrap(), MIN_MULTIPART_PART_SIZE);
        let maximum_object = MAX_MULTIPART_PART_SIZE * MAX_MULTIPART_PARTS;
        assert_eq!(multipart_part_size(maximum_object).unwrap(), MAX_MULTIPART_PART_SIZE);
        assert!(multipart_part_size(maximum_object + 1).is_err());
    }

    #[tokio::test]
    async fn live_s3_multipart_unicode_pagination_and_abort() {
        let Ok(endpoint) = std::env::var("S3_TEST_ENDPOINT") else { return };
        let access_key_id = std::env::var("S3_TEST_ACCESS_KEY").expect("S3_TEST_ACCESS_KEY");
        let secret_access_key = std::env::var("S3_TEST_SECRET_KEY").expect("S3_TEST_SECRET_KEY");
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let bucket = format!("harbor-transfer-{}-{timestamp}", std::process::id());
        let config = S3Config {
            region: "us-east-1".into(),
            bucket: bucket.clone(),
            endpoint: Some(endpoint),
            access_key_id,
            secret_access_key,
            session_token: None,
            force_path_style: true,
            probe_path: "/".into(),
        };
        let mut s3 = S3Client::connect_insecure_local_for_test(&config).await.expect("test client");
        s3.client.create_bucket().bucket(&bucket).send().await.expect("create test bucket");

        let workspace = tempfile::tempdir().expect("temporary S3 files");
        let source = workspace.path().join("source.bin");
        let payload: Vec<u8> =
            (0..(MIN_MULTIPART_PART_SIZE * 2 + 257)).map(|index| (index % 251) as u8).collect();
        tokio::fs::write(&source, &payload).await.expect("write multipart source");
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_events = progress.clone();
        let uploaded = s3
            .upload_file_with_progress(
                &source.to_string_lossy(),
                "/ページ/海の写真.bin",
                move |done, total| {
                    progress_events.lock().unwrap().push((done, total));
                    async { Ok(()) }
                },
            )
            .await
            .expect("multipart upload");
        assert_eq!(uploaded, payload.len() as u64);
        assert!(progress.lock().unwrap().len() >= 3);

        let listed = s3.list_dir("/ページ").await.expect("list Unicode prefix");
        assert!(listed.iter().any(|entry| entry.name == "海の写真.bin" && entry.size == uploaded));
        let destination = workspace.path().join("downloaded.bin");
        let downloaded = s3
            .download_file("/ページ/海の写真.bin", &destination.to_string_lossy())
            .await
            .expect("streaming download");
        assert_eq!(downloaded, uploaded);
        assert_eq!(tokio::fs::read(destination).await.unwrap(), payload);

        let calls = Arc::new(AtomicUsize::new(0));
        let cancellation_calls = calls.clone();
        let cancelled = s3
            .upload_file_with_progress(&source.to_string_lossy(), "/cancelled.bin", move |_, _| {
                let call = cancellation_calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if call == 0 {
                        Err(anyhow!("cancel integration test"))
                    } else {
                        Ok(())
                    }
                }
            })
            .await;
        assert!(cancelled.is_err());
        let pending = s3
            .client
            .list_multipart_uploads()
            .bucket(&bucket)
            .prefix("cancelled.bin")
            .send()
            .await
            .expect("list incomplete uploads");
        assert!(pending.uploads().is_empty(), "cancelled multipart upload was not aborted");

        let raw_client = s3.client.clone();
        let uploads = stream::iter(0..1001usize)
            .map(|index| {
                let client = raw_client.clone();
                let bucket = bucket.clone();
                async move {
                    client
                        .put_object()
                        .bucket(bucket)
                        .key(format!("many/item-{index:04}.txt"))
                        .body(ByteStream::from_static(&[]))
                        .send()
                        .await
                }
            })
            .buffer_unordered(32)
            .collect::<Vec<_>>()
            .await;
        assert!(uploads.into_iter().all(|result| result.is_ok()));
        let paged = s3.list_dir("/many").await.expect("paginated listing");
        assert_eq!(paged.len(), 1001);
        assert_eq!(paged.first().unwrap().name, "item-0000.txt");
        assert_eq!(paged.last().unwrap().name, "item-1000.txt");
    }
}

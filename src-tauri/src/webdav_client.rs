use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use percent_encoding::percent_decode_str;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::{Client, Method, StatusCode, Url};
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

use crate::sftp_client::{FileEntry, FileEntryType};

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/><d:getlastmodified/></d:prop></d:propfind>"#;

#[derive(Debug, Clone)]
pub struct WebDavConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub probe_path: String,
}

pub struct WebDavClient {
    client: Client,
    base_url: Url,
    username: String,
    password: String,
}

impl WebDavClient {
    pub async fn connect(config: &WebDavConfig) -> Result<Self> {
        let host = config.host.trim();
        if host.is_empty() || host.contains('/') || host.contains('@') {
            return Err(anyhow!("WebDAV server must be a hostname or IP address without a URL path."));
        }
        let base_url = Url::parse(&format!("https://{host}:{}/", config.port))
            .map_err(|error| anyhow!("Invalid WebDAV HTTPS endpoint: {error}"))?;
        let builder = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("Harbor-Transfer/", env!("CARGO_PKG_VERSION")));
        #[cfg(test)]
        let builder = match std::env::var("WEBDAV_TEST_CA_CERT") {
            Ok(path) => {
                let pem = std::fs::read(&path)
                    .map_err(|error| anyhow!("Could not read WEBDAV_TEST_CA_CERT '{path}': {error}"))?;
                let certificate = reqwest::Certificate::from_pem(&pem)
                    .map_err(|error| anyhow!("Invalid WEBDAV_TEST_CA_CERT '{path}': {error}"))?;
                builder.add_root_certificate(certificate)
            }
            Err(_) => builder,
        };
        let client = builder.build().map_err(|error| anyhow!("Could not configure WebDAV HTTPS: {error}"))?;
        let connected =
            Self { client, base_url, username: config.username.clone(), password: config.password.clone() };
        connected.propfind(&config.probe_path, "0").await?;
        Ok(connected)
    }

    fn url(&self, path: &str) -> Result<Url> {
        if !path.starts_with('/') || path.split('/').any(|component| component == "..") {
            return Err(anyhow!("WebDAV paths must be absolute and cannot contain '..'."));
        }
        let mut url = self.base_url.clone();
        url.set_path(path);
        Ok(url)
    }

    fn authenticated(&self, method: Method, url: Url) -> reqwest::RequestBuilder {
        self.client.request(method, url).basic_auth(&self.username, Some(&self.password))
    }

    async fn propfind(&self, path: &str, depth: &str) -> Result<String> {
        let method = Method::from_bytes(b"PROPFIND").expect("constant WebDAV method");
        let response = self
            .authenticated(method, self.url(path)?)
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(PROPFIND_BODY)
            .send()
            .await
            .map_err(|error| anyhow!("WebDAV PROPFIND failed: {error}"))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(anyhow!("WebDAV authentication failed."));
        }
        if response.status() != StatusCode::MULTI_STATUS {
            return Err(anyhow!("WebDAV PROPFIND returned HTTP {}.", response.status()));
        }
        response.text().await.map_err(|error| anyhow!("Could not read WebDAV response: {error}"))
    }

    pub async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>> {
        let xml = self.propfind(path, "1").await?;
        parse_multistatus(&xml, path)
    }

    pub async fn upload_file(&self, local_path: &str, remote_path: &str) -> Result<u64> {
        let file = tokio::fs::File::open(local_path)
            .await
            .map_err(|error| anyhow!("Could not open local file '{}': {error}", local_path))?;
        let size = file.metadata().await?.len();
        let response = self
            .authenticated(Method::PUT, self.url(remote_path)?)
            .header(reqwest::header::CONTENT_LENGTH, size)
            .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
            .send()
            .await
            .map_err(|error| anyhow!("WebDAV upload failed: {error}"))?;
        ensure_success("PUT", response.status())?;
        Ok(size)
    }

    pub async fn download_file(&self, remote_path: &str, local_path: &str) -> Result<u64> {
        let response = self
            .authenticated(Method::GET, self.url(remote_path)?)
            .send()
            .await
            .map_err(|error| anyhow!("WebDAV download failed: {error}"))?;
        ensure_success("GET", response.status())?;
        let mut stream = response.bytes_stream();
        let mut output = tokio::fs::File::create(local_path).await?;
        let mut written = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| anyhow!("WebDAV download stream failed: {error}"))?;
            output.write_all(&chunk).await?;
            written += chunk.len() as u64;
        }
        output.flush().await?;
        Ok(written)
    }

    pub async fn create_dir(&self, path: &str) -> Result<()> {
        let method = Method::from_bytes(b"MKCOL").expect("constant WebDAV method");
        let response = self.authenticated(method, self.url(path)?).send().await?;
        if response.status() == StatusCode::METHOD_NOT_ALLOWED {
            return Ok(());
        }
        ensure_success("MKCOL", response.status())
    }

    pub async fn rename(&self, old_path: &str, new_path: &str) -> Result<()> {
        let method = Method::from_bytes(b"MOVE").expect("constant WebDAV method");
        let destination = self.url(new_path)?;
        let response = self
            .authenticated(method, self.url(old_path)?)
            .header("Destination", destination.as_str())
            .header("Overwrite", "F")
            .send()
            .await?;
        ensure_success("MOVE", response.status())
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let response = self.authenticated(Method::DELETE, self.url(path)?).send().await?;
        ensure_success("DELETE", response.status())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        Ok(())
    }
}

fn ensure_success(operation: &str, status: StatusCode) -> Result<()> {
    if status.is_success() {
        Ok(())
    } else {
        Err(anyhow!("WebDAV {operation} returned HTTP {status}."))
    }
}

#[derive(Default)]
struct DavResponse {
    href: String,
    size: u64,
    modified: Option<String>,
    directory: bool,
}

fn parse_multistatus(xml: &str, requested_path: &str) -> Result<Vec<FileEntry>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut current: Option<DavResponse> = None;
    let mut text_target = "";
    let mut responses = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(element) => match element.local_name().as_ref() {
                b"response" => current = Some(DavResponse::default()),
                b"href" => text_target = "href",
                b"getcontentlength" => text_target = "size",
                b"getlastmodified" => text_target = "modified",
                b"collection" => {
                    if let Some(response) = current.as_mut() {
                        response.directory = true;
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some(response) = current.as_mut() {
                    let value = text.decode()?.into_owned();
                    match text_target {
                        "href" => response.href = value,
                        "size" => response.size = value.parse().unwrap_or(0),
                        "modified" => response.modified = Some(value),
                        _ => {}
                    }
                }
            }
            Event::End(element) => match element.local_name().as_ref() {
                b"response" => {
                    if let Some(response) = current.take() {
                        responses.push(response);
                    }
                }
                b"href" | b"getcontentlength" | b"getlastmodified" => text_target = "",
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    let requested = normalize_path(requested_path);
    let mut entries = Vec::new();
    for response in responses {
        let encoded_path =
            Url::parse(&response.href).ok().map(|url| url.path().to_string()).unwrap_or(response.href);
        let decoded = percent_decode_str(&encoded_path).decode_utf8_lossy();
        let path = normalize_path(&decoded);
        if path == requested {
            continue;
        }
        let Some(name) = Path::new(&path).file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        entries.push(FileEntry {
            name: name.to_string(),
            path_component: None,
            download_name: None,
            size: response.size,
            modified: response.modified,
            permissions: None,
            file_type: if response.directory { FileEntryType::Directory } else { FileEntryType::File },
            owner: None,
            group: None,
        });
    }
    entries.sort_by(|left, right| {
        let left_dir = matches!(left.file_type, FileEntryType::Directory);
        let right_dir = matches!(right.file_type, FileEntryType::Directory);
        right_dir.cmp(&left_dir).then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(entries)
}

fn normalize_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_namespaced_unicode_multistatus_and_skips_parent() {
        let xml = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:">
          <d:response><d:href>/docs/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
          <d:response><d:href>/docs/%E6%B8%AF.txt</d:href><d:propstat><d:prop><d:getcontentlength>42</d:getcontentlength><d:getlastmodified>Tue, 25 Aug 2026 10:00:00 GMT</d:getlastmodified><d:resourcetype/></d:prop></d:propstat></d:response>
        </d:multistatus>"#;
        let entries = parse_multistatus(xml, "/docs").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "港.txt");
        assert_eq!(entries[0].size, 42);
    }

    #[tokio::test]
    async fn live_webdav_crud_unicode_empty_collection_and_streaming() {
        let Ok(host) = std::env::var("WEBDAV_TEST_HOST") else {
            eprintln!("SKIP: WEBDAV_TEST_HOST not set");
            return;
        };
        let port =
            std::env::var("WEBDAV_TEST_PORT").ok().and_then(|value| value.parse().ok()).unwrap_or(8443);
        let root = std::env::var("WEBDAV_TEST_ROOT").unwrap_or_else(|_| "/".into());
        let config = WebDavConfig {
            host,
            port,
            username: std::env::var("WEBDAV_TEST_USER").unwrap_or_else(|_| "harbor".into()),
            password: std::env::var("WEBDAV_TEST_PASS").unwrap_or_else(|_| "harbor".into()),
            probe_path: root.clone(),
        };
        let mut invalid = config.clone();
        invalid.password = "definitely-wrong".into();
        assert!(WebDavClient::connect(&invalid).await.is_err(), "invalid credentials must fail");
        let mut client = WebDavClient::connect(&config).await.expect("connect WebDAV");
        let directory = format!("{}/harbor_空フォルダ", root.trim_end_matches('/'));
        let directory = if directory.starts_with('/') { directory } else { format!("/{directory}") };
        let remote = format!("{directory}/港便り.bin");
        let renamed = format!("{directory}/港便り_確認済み.bin");
        let _ = client.delete(&renamed).await;
        let _ = client.delete(&remote).await;
        let _ = client.delete(&directory).await;
        client.create_dir(&directory).await.expect("create collection");
        assert!(client.list_dir(&directory).await.expect("list empty collection").is_empty());

        let upload = std::env::temp_dir().join("harbor_webdav_upload.bin");
        let download = std::env::temp_dir().join("harbor_webdav_download.bin");
        let content = vec![0x57; 2 * 1024 * 1024 + 17];
        tokio::fs::write(&upload, &content).await.unwrap();
        client.upload_file(upload.to_str().unwrap(), &remote).await.expect("upload");
        client.rename(&remote, &renamed).await.expect("move");
        client.download_file(&renamed, download.to_str().unwrap()).await.expect("download");
        assert_eq!(tokio::fs::read(&download).await.unwrap(), content);
        client.delete(&renamed).await.expect("delete file");
        client.delete(&directory).await.expect("delete collection");
        let _ = tokio::fs::remove_file(upload).await;
        let _ = tokio::fs::remove_file(download).await;
        client.disconnect().await.unwrap();
        let mut recovered = WebDavClient::connect(&config).await.expect("reconnect after disconnect");
        recovered.list_dir(&root).await.expect("list after reconnect");
        recovered.disconnect().await.unwrap();
    }
}

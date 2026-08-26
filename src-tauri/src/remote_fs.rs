use crate::ftp_client::FtpClient;
use crate::s3_client::S3Client;
use crate::sftp_client::{FileEntry, StandaloneSftpClient};
use crate::webdav_client::WebDavClient;
use anyhow::Result;
use async_trait::async_trait;

/// Protocol-neutral remote file operations used by browsing, transfers, and sync planning.
#[async_trait]
pub trait RemoteFileSystem: Send {
    async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>>;
    async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> Result<u64>;
    async fn download_file(&mut self, remote_path: &str, local_path: &str) -> Result<u64>;
    async fn create_dir(&mut self, path: &str) -> Result<()>;
    async fn rename(&mut self, old_path: &str, new_path: &str) -> Result<()>;
    async fn set_metadata(
        &mut self,
        path: &str,
        permissions: Option<u32>,
        modified: Option<u32>,
    ) -> Result<()>;
    async fn delete_file(&mut self, path: &str) -> Result<()>;
    async fn delete_dir(&mut self, path: &str) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
}

#[async_trait]
impl RemoteFileSystem for StandaloneSftpClient {
    async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>> {
        StandaloneSftpClient::list_dir(self, path).await
    }
    async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> Result<u64> {
        StandaloneSftpClient::upload_file(self, local_path, remote_path).await
    }
    async fn download_file(&mut self, remote_path: &str, local_path: &str) -> Result<u64> {
        StandaloneSftpClient::download_file(self, remote_path, local_path).await
    }
    async fn create_dir(&mut self, path: &str) -> Result<()> {
        StandaloneSftpClient::create_dir(self, path).await
    }
    async fn rename(&mut self, old_path: &str, new_path: &str) -> Result<()> {
        StandaloneSftpClient::rename(self, old_path, new_path).await
    }
    async fn set_metadata(
        &mut self,
        path: &str,
        permissions: Option<u32>,
        modified: Option<u32>,
    ) -> Result<()> {
        StandaloneSftpClient::set_metadata(self, path, permissions, modified).await
    }
    async fn delete_file(&mut self, path: &str) -> Result<()> {
        StandaloneSftpClient::delete_file(self, path).await
    }
    async fn delete_dir(&mut self, path: &str) -> Result<()> {
        StandaloneSftpClient::delete_dir(self, path).await
    }
    async fn disconnect(&mut self) -> Result<()> {
        StandaloneSftpClient::disconnect(self).await
    }
}

#[async_trait]
impl RemoteFileSystem for FtpClient {
    async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>> {
        FtpClient::list_dir(self, path).await
    }
    async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> Result<u64> {
        FtpClient::upload_file(self, local_path, remote_path).await
    }
    async fn download_file(&mut self, remote_path: &str, local_path: &str) -> Result<u64> {
        FtpClient::download_file(self, remote_path, local_path).await
    }
    async fn create_dir(&mut self, path: &str) -> Result<()> {
        FtpClient::create_dir(self, path).await
    }
    async fn rename(&mut self, old_path: &str, new_path: &str) -> Result<()> {
        FtpClient::rename(self, old_path, new_path).await
    }
    async fn set_metadata(
        &mut self,
        path: &str,
        permissions: Option<u32>,
        modified: Option<u32>,
    ) -> Result<()> {
        FtpClient::set_metadata(self, path, permissions, modified).await
    }
    async fn delete_file(&mut self, path: &str) -> Result<()> {
        FtpClient::delete_file(self, path).await
    }
    async fn delete_dir(&mut self, path: &str) -> Result<()> {
        FtpClient::delete_dir(self, path).await
    }
    async fn disconnect(&mut self) -> Result<()> {
        FtpClient::disconnect(self).await
    }
}

#[async_trait]
impl RemoteFileSystem for WebDavClient {
    async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>> {
        WebDavClient::list_dir(self, path).await
    }
    async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> Result<u64> {
        WebDavClient::upload_file(self, local_path, remote_path).await
    }
    async fn download_file(&mut self, remote_path: &str, local_path: &str) -> Result<u64> {
        WebDavClient::download_file(self, remote_path, local_path).await
    }
    async fn create_dir(&mut self, path: &str) -> Result<()> {
        WebDavClient::create_dir(self, path).await
    }
    async fn rename(&mut self, old_path: &str, new_path: &str) -> Result<()> {
        WebDavClient::rename(self, old_path, new_path).await
    }
    async fn set_metadata(
        &mut self,
        _path: &str,
        _permissions: Option<u32>,
        _modified: Option<u32>,
    ) -> Result<()> {
        anyhow::bail!("WebDAV does not provide portable POSIX permission or modification-date changes.")
    }
    async fn delete_file(&mut self, path: &str) -> Result<()> {
        WebDavClient::delete(self, path).await
    }
    async fn delete_dir(&mut self, path: &str) -> Result<()> {
        WebDavClient::delete(self, path).await
    }
    async fn disconnect(&mut self) -> Result<()> {
        WebDavClient::disconnect(self).await
    }
}

#[async_trait]
impl RemoteFileSystem for S3Client {
    async fn list_dir(&mut self, path: &str) -> Result<Vec<FileEntry>> {
        S3Client::list_dir(self, path).await
    }
    async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> Result<u64> {
        S3Client::upload_file(self, local_path, remote_path).await
    }
    async fn download_file(&mut self, remote_path: &str, local_path: &str) -> Result<u64> {
        S3Client::download_file(self, remote_path, local_path).await
    }
    async fn create_dir(&mut self, path: &str) -> Result<()> {
        S3Client::create_dir(self, path).await
    }
    async fn rename(&mut self, old_path: &str, new_path: &str) -> Result<()> {
        S3Client::rename(self, old_path, new_path).await
    }
    async fn set_metadata(
        &mut self,
        _path: &str,
        _permissions: Option<u32>,
        _modified: Option<u32>,
    ) -> Result<()> {
        anyhow::bail!("S3 objects do not expose POSIX permissions or a writable Last-Modified value.")
    }
    async fn delete_file(&mut self, path: &str) -> Result<()> {
        S3Client::delete_file(self, path).await
    }
    async fn delete_dir(&mut self, path: &str) -> Result<()> {
        S3Client::delete_dir(self, path).await
    }
    async fn disconnect(&mut self) -> Result<()> {
        S3Client::disconnect(self).await
    }
}

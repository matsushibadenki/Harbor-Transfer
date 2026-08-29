# Google Cloud FTP integration design

## Scope

Harbor Transfer treats Google Cloud FTP as a distinct connection type while reusing the existing SFTP transport. Cloud FTP exposes Cloud Storage through an SFTP endpoint; it is not an FTP or FTPS endpoint.

## Connection model

- The bookmark protocol value is `cloudFtp` and the default port is 22.
- Users enter the Cloud FTP hostname or external IP address and the Cloud FTP username.
- SSH public-key authentication is mandatory. Harbor Transfer requires the private key paired with the public key registered for the Cloud FTP user.
- Existing SSH host-key verification, encrypted-key passphrase prompts, and macOS Keychain storage are reused.
- Password authentication is rejected in both the UI and Rust command layer.

## Supported operations

- List directories; upload and download files and folders.
- Create directories and delete files or empty directories.
- Rename files. Directory rename remains server-dependent and requires a Cloud Storage bucket with hierarchical namespace enabled.
- Finder drag export, external-editor cache, transfer queue, and safe one-way sync reuse the SFTP implementation.

## Capability restrictions

Cloud Storage IAM controls access. Cloud FTP does not provide portable operations for changing POSIX permissions, owner, group, or modification time. The file-information panel therefore keeps rename available but disables these metadata controls. The Rust command layer rejects metadata mutation as a second safety boundary.

## Errors and diagnostics

- A missing private key produces a Cloud FTP-specific message before connection.
- Authentication, host-key, timeout, and network failures continue to use the established SFTP diagnostics.
- Server rejections for unsupported rename or directory behavior are surfaced without being converted into a successful operation.

## Test boundary

Unit and type checks cover the stable `cloudFtp` wire value and all shared SFTP code. A live Cloud FTP integration test requires a user-owned Google Cloud project, server, bucket, service account, user mapping, and SSH public key, so it remains opt-in and must not place credentials in the repository or CI logs.

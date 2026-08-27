# Additional protocol evaluation

## Phase 5 result

WebDAV is implemented through the existing `RemoteFileSystem` operations and reuses the list, transfer, drag-out, external-editing, and synchronization UI. The adapter uses `reqwest` with Rustls, accepts HTTPS endpoints only, validates TLS certificates, parses namespace-qualified multistatus responses, decodes percent-encoded Unicode paths, and streams file bodies instead of buffering whole transfers.

Phase 5 supports HTTP Basic authentication only over mandatory HTTPS. Credentials use the existing macOS Keychain path and never enter bookmark exports or local diagnostics. Digest, OAuth, and client-certificate authentication remain optional future compatibility work and are not silently downgraded.

S3 is the Phase 6 target. Object storage has no native directories, rename is copy plus delete, and multipart upload changes progress and cancellation semantics, so its safety and credential model will be designed before write operations are enabled.

## Acceptance gate for WebDAV

- HTTPS certificate validation remains enabled.
- `PROPFIND`, download, upload, collection creation, move, and delete map cleanly to `RemoteFileSystem`.
- Percent-encoded Unicode paths and empty collections pass containerized integration tests.
- The UI states clearly when a server does not support an optional capability.
- Credentials remain in macOS Keychain and never enter bookmark exports or diagnostics.

All gates above are covered by Rust unit tests and containerized live tests against both a standards-focused HTTPS fixture and Nextcloud.

## Phase 8 result

Samba support uses the pure-Rust [`smb2`](https://github.com/vdavid/smb2) client. Version 0.20 provides an SMB 2/3 client without linking to the system Samba libraries and supports NTLM authentication, signing, encryption, DFS, streaming I/O, and automatic reconnect. Harbor Transfer does not implement an SMB 1 fallback.

Each bookmark identifies one server share. It stores the server, port, share name, initial directory, optional workgroup/domain, and whether guest authentication is enabled. User passwords reuse the existing macOS Keychain service and are excluded from SQLite bookmark records, JSON exports, and diagnostics. SMB paths are normalized as share-relative paths; parent traversal and destructive operations against the share root are rejected.

The adapter implements directory listing, streaming upload/download, directory creation, rename, file/directory deletion, modification-date updates, and disconnect through `RemoteFileSystem`. The common command layer consequently supplies recursive folder transfer, Finder drag-out, cached external editing, copy/move fallback, synchronization preview/execution, conflict handling, and transfer history. POSIX owner, group, and permission fields do not have a portable SMB mapping and remain unavailable. Modification dates use the crate's public low-level protocol types to run a signed short-lived `CREATE → SET_INFO (FileBasicInformation) → CLOSE` sequence without requiring a system Samba command or a private crate fork.

## Acceptance gate for Samba

- Only SMB 2/3 is negotiated; no SMB 1 compatibility fallback is present.
- Passwords stay in macOS Keychain and guest connections do not retain a credential.
- Share names and remote paths reject separators, parent traversal, NUL characters, and destructive root operations.
- Unicode paths, empty directories, multi-megabyte streaming transfers, and reconnect pass the containerized live test.
- Permission-denied behavior, progress reporting, cancellation cleanup, and reconnect are covered by the containerized live test.
- File and directory modification-date updates are covered by the containerized live test.

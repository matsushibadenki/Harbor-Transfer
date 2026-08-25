# Additional protocol evaluation

## Decision

WebDAV is the preferred next protocol. It maps naturally to the existing `RemoteFileSystem` operations, is widely available on self-hosted and managed storage, and can reuse the current list/transfer/synchronization UI. The adapter should use a maintained Rust HTTP client, validate TLS by default, support Basic and Digest authentication, parse multistatus responses defensively, and test Nextcloud plus a standards-focused WebDAV server.

S3 remains a later target. Object storage has no native directories, rename is copy plus delete, eventual consistency and multipart upload change progress/cancellation semantics, and AWS credential handling needs a dedicated secure design. Treating it as a filesystem prematurely would make destructive synchronization ambiguous.

## Acceptance gate for WebDAV

- HTTPS certificate validation remains enabled.
- `PROPFIND`, download, upload, collection creation, copy/move, and delete map cleanly to `RemoteFileSystem`.
- Percent-encoded Unicode paths and empty collections pass containerized integration tests.
- The UI states clearly when a server does not support an optional capability.
- Credentials remain in macOS Keychain and never enter bookmark exports or diagnostics.

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

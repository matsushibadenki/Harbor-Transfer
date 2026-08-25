# Quality and release readiness

## Automated quality gates

The `CI` workflow runs TypeScript type checking, the production frontend build, Rust formatting, all Rust unit tests, protocol integration tests, and a debug macOS application bundle. Pull requests cannot rely on a developer's configured servers.

Run the same checks locally:

```sh
pnpm check
pnpm build
pnpm test:integration
```

The integration environment starts isolated FTP, Explicit FTPS, SFTP, standards-focused HTTPS WebDAV, and Nextcloud servers. Its CRUD cycles cover authentication failure, disconnect and reconnect, empty directories, UTF-8 and percent-encoded paths, rename/delete cleanup, certificate validation, and a 2 MiB + 17 byte binary transfer. The size is intentionally CI-friendly while still crossing the application's streaming buffers many times. Running the same WebDAV suite against the purpose-built fixture and Nextcloud guards both protocol behavior and real-server interoperability.

## Accessibility and interaction audit

- Every interactive file row is keyboard-focusable. `Space` selects it and `Enter` opens or downloads it.
- Focus indicators remain visible for buttons, fields, file rows, and resize controls.
- Icon-only navigation and action buttons have localized accessible names in English, Japanese, and Simplified Chinese.
- `Command/Ctrl + ,` opens Preferences, `Command/Ctrl + N` opens New Connection, and `Command/Ctrl + R` refreshes the active directory.
- Status notices use a polite live region; selection and pressed states are exposed to assistive technology.
- Light, dark, and system-following themes are available in Preferences. The dark palette was checked for surface hierarchy, focus visibility, selection visibility, and readable muted text.

## Manual release checklist

Before publishing a release, test the smallest supported window, a large bookmark/key collection, all three languages, VoiceOver keyboard navigation, light/dark appearance, invalid credentials and certificates, loss of network during a transfer, folder drag-out to Finder, cached external editing, and a representative production WebDAV endpoint.

No passwords, passphrases, private keys, local paths, or server addresses may be added to screenshots, logs, artifacts, or bug reports.

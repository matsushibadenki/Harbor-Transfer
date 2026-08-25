# macOS release procedure

## Signing and notarization

Harbor Transfer uses the stable bundle identifier `com.harbortransfer.desktop`. On first launch it safely copies an existing pre-release `com.harbortransfer.app` bookmark database when the new data directory is empty; Keychain entries retain their product-scoped service name. Store Apple credentials outside the repository and run:

```sh
APPLE_SIGNING_IDENTITY='Developer ID Application: …' \
APPLE_ID='release@example.com' \
APPLE_PASSWORD='app-specific-password' \
APPLE_TEAM_ID='TEAMID' \
sh scripts/release-macos.sh
```

Tauri performs code signing and notarization when those standard environment variables are present. Verify the resulting application and disk image before publishing:

```sh
codesign --verify --deep --strict --verbose=2 'src-tauri/target/release/bundle/macos/Harbor Transfer.app'
spctl --assess --type execute --verbose=2 'src-tauri/target/release/bundle/macos/Harbor Transfer.app'
xcrun stapler validate 'src-tauri/target/release/bundle/dmg/'*.dmg
```

## Update delivery policy

Until a signed update endpoint and offline recovery path are operated, releases are delivered as notarized DMGs from the project's GitHub Releases page. Do not enable an updater with placeholder keys or an unsigned feed. A future automatic updater must use a separate Tauri updater signing key, HTTPS, a staged channel, rollback instructions, and a manually downloadable DMG for recovery.

## Crash diagnostics and privacy

The application keeps at most five local panic reports in its application-data `diagnostics` directory. Reports contain only the application version, Unix timestamp, and Rust source location. Panic payloads, credentials, hosts, and user file paths are deliberately omitted. Reports are never uploaded automatically; a user must inspect and attach one explicitly.

Release operators must strip credentials from CI output and keep signing identities and Apple app-specific passwords in protected repository secrets.

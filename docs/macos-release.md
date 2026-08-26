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

## GitHub Release deployment

The `Release macOS` workflow builds a universal application for Apple Silicon and Intel Macs, signs it with a Developer ID Application certificate, submits it to Apple for notarization, staples the ticket, and publishes the DMG to GitHub Releases. It runs only when manually dispatched and fails before building if a required secret is absent.

The Apple Team ID is configured as `3WH28SSRZC`. The product name is `Harbor Transfer`, the GitHub release tag is derived from the Tauri version (for example `v0.1.0`), and the stable macOS bundle identifier remains `com.harbortransfer.desktop`. `Harbor-Transfer` is suitable as a project or release name, but it is not a Developer ID signing identity. Apple assigns the identity in the form `Developer ID Application: Account Name (3WH28SSRZC)`.

Configure these repository secrets under **Settings → Secrets and variables → Actions**:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` export containing the Developer ID Application certificate and private key |
| `APPLE_CERTIFICATE_PASSWORD` | Password chosen when exporting the `.p12` file |
| `APPLE_ID` | Apple account email used for notarization |
| `APPLE_PASSWORD` | Apple app-specific password, not the normal Apple account password |

Create the certificate in the Apple Developer portal using the **Developer ID Application** type. After importing the issued certificate into Keychain Access, export it together with its private key as a password-protected `.p12`. Encode it on macOS with:

```sh
openssl base64 -A -in DeveloperIDApplication.p12
```

Copy the output directly into `APPLE_CERTIFICATE`; never commit the `.p12`, its password, or the app-specific password. Once all secrets exist, open **Actions → Release macOS → Run workflow**. Version `0.1.0` is published as tag `v0.1.0`; increase both the Tauri and Cargo package versions before a later release.

## Update delivery policy

Until a signed update endpoint and offline recovery path are operated, releases are delivered as notarized DMGs from the project's GitHub Releases page. Do not enable an updater with placeholder keys or an unsigned feed. A future automatic updater must use a separate Tauri updater signing key, HTTPS, a staged channel, rollback instructions, and a manually downloadable DMG for recovery.

## Crash diagnostics and privacy

The application keeps at most five local panic reports in its application-data `diagnostics` directory. Reports contain only the application version, Unix timestamp, and Rust source location. Panic payloads, credentials, hosts, and user file paths are deliberately omitted. Reports are never uploaded automatically; a user must inspect and attach one explicitly.

Release operators must strip credentials from CI output and keep signing identities and Apple app-specific passwords in protected repository secrets.

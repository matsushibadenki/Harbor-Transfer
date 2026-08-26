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
| `TAURI_SIGNING_PRIVATE_KEY` | Private key dedicated to signing updater archives |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the updater signing private key |

Create the certificate in the Apple Developer portal using the **Developer ID Application** type. After importing the issued certificate into Keychain Access, export it together with its private key as a password-protected `.p12`. Encode it on macOS with:

```sh
openssl base64 -A -in DeveloperIDApplication.p12
```

Copy the output directly into `APPLE_CERTIFICATE`; never commit the `.p12`, either private key, or any password. Once all secrets exist, open **Actions → Release macOS → Run workflow**. The workflow creates the signed universal application, DMG, updater archive and signature, plus `latest.json` for in-app update discovery. Increase the package, Tauri, and Cargo versions together before every release.

## Update delivery policy

Harbor Transfer checks the HTTPS `latest.json` published with the newest GitHub Release. The updater verifies every downloaded archive with the dedicated public key embedded in the application before installation. Keep the notarized DMG attached to every release as an offline recovery and manual-install path. Version `0.2.0` is the first updater-capable build, so users of `0.1.0` must install `0.2.0` manually once; later releases can update in-app.

## Crash diagnostics and privacy

The application keeps at most five local panic reports in its application-data `diagnostics` directory. Reports contain only the application version, Unix timestamp, and Rust source location. Panic payloads, credentials, hosts, and user file paths are deliberately omitted. Reports are never uploaded automatically; a user must inspect and attach one explicitly.

Release operators must strip credentials from CI output and keep signing identities and Apple app-specific passwords in protected repository secrets.

# Harbor Transfer

<p align="center">
  <img src="docs/images/charactor1.png" width="220" alt="Harbor Transfer seal courier mascot">
</p>

Harbor Transfer is a friendly desktop file-transfer client built with **Tauri 2**, **Rust**, and **React**. It provides a Finder-like interface for browsing remote servers, managing saved connections, and tracking transfers without hiding important security details.

> Harbor Transfer is under active development and is not yet a production release.

## Application Preview

![Harbor Transfer main window](docs/images/Harbor-Transfer_image.png)

## Features

- Connect to remote servers using **SFTP**, **FTP**, or **Explicit FTPS**.
- Authenticate with a password or an SSH private key.
- Verify SSH host-key fingerprints on first connection and reject unexpected changes.
- Browse remote files in list, icon, or Finder-style column views.
- Navigate with breadcrumbs, direct path entry, parent-folder controls, refresh, and search.
- Display file size, modification date, and permissions when supported by the protocol.
- Upload files, multiple selections, and complete folders, including Finder drag and drop.
- Download, rename, delete, and create remote folders.
- Monitor transfers with progress, speed, remaining time, pause, resume, cancel, and retry controls.
- Resolve duplicate filenames by asking, overwriting, skipping, or choosing another name.
- Save, edit, tag, import, and export bookmarks.
- Associate an optional local directory with each bookmark for future differential synchronization.
- Preview one-way differences between local and remote directory trees.
- Review and clear connection and transfer history.
- Manage available SSH keys from the local `~/.ssh` directory.
- Use the interface in English, Japanese, or Simplified Chinese.

## Security and Privacy

- Passwords are stored in the macOS Keychain and are never written to the bookmark database.
- Bookmark exports do not contain passwords, private-key contents, or passphrases.
- SSH private-key contents are not exposed through the user interface.
- FTPS certificate verification remains enabled by default.
- Remote paths are handled as structured application data and are not executed as shell commands.

## Technology

- Tauri 2
- Rust and Tokio
- React and TypeScript
- SQLite for bookmarks and history
- macOS Keychain for passwords
- `russh` / `russh-sftp` for SFTP
- `suppaftp` for FTP and FTPS

## Development

### Requirements

- macOS
- Rust toolchain
- Node.js
- pnpm
- Tauri 2 system prerequisites

### Install dependencies

```bash
pnpm install
```

### Run the application

```bash
pnpm tauri dev
```

### Run checks

```bash
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
```

### Build the application

```bash
pnpm tauri build
```

## Project Status

Bookmark management, remote browsing, file operations, transfer controls, FTP/FTPS support, and synchronization preview are implemented. Safe synchronization execution, exclusion rules, execution logs, packaging, signing, and automatic updates remain planned work.

See [the roadmap](docs/roadmap.md) and [the functional design](docs/functional-design.md) for more detail.

#!/bin/sh
set -eu

: "${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY to a Developer ID Application identity}"
: "${APPLE_ID:?Set APPLE_ID for notarization}"
: "${APPLE_PASSWORD:?Set APPLE_PASSWORD to an app-specific password}"
: "${APPLE_TEAM_ID:?Set APPLE_TEAM_ID to the Apple Developer team identifier}"

pnpm install --frozen-lockfile
pnpm check
pnpm tauri build --bundles app,dmg

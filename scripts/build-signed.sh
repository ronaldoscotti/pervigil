#!/usr/bin/env bash
# Signed + notarized release build. Sources .env for the Apple credentials
# (APPLE_SIGNING_IDENTITY, APPLE_TEAM_ID, APPLE_ID, APPLE_PASSWORD), then runs the
# bundle. Without those set, `tauri build` still produces an unsigned bundle.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  echo "warning: APPLE_SIGNING_IDENTITY unset — building UNSIGNED." >&2
fi

# externalBin lives in a bundle-only config so plain `cargo test` / `cargo build`
# (which run tauri-build) don't require the staged shim; the merge happens only here.
npm run tauri build -- --config "$(pwd)/src-tauri/tauri.bundle.conf.json" "$@"

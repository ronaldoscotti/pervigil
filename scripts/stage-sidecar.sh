#!/usr/bin/env bash
# Build the `record` shim and stage it under the target-triple name Tauri's
# `externalBin` expects, so it ships inside the app bundle beside the main binary.
# Runs before both `tauri dev` and `tauri build`.
set -euo pipefail

cd "$(dirname "$0")/../src-tauri"

profile="${1:-debug}"
triple="$(rustc -vV | sed -n 's/^host: //p')"

# The shim lives in the app crate, so building it runs tauri-build, which refuses to
# compile while its own `externalBin` target is missing. Break the cycle with an empty
# placeholder the build accepts, then overwrite it with the real binary.
mkdir -p binaries
touch "binaries/record-$triple"

if [ "$profile" = "release" ]; then
  cargo build --release --bin record
else
  cargo build --bin record
fi

cp "target/$profile/record" "binaries/record-$triple"
echo "staged binaries/record-$triple ($profile)"

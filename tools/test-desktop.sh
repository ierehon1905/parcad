#!/usr/bin/env bash
# Compile and exercise the desktop application, including the isolated OCCT
# worker. Browser-only tests are intentionally insufficient for this suite:
# its purpose is to catch failures in Tauri's WebKit host and IPC bridge.
set -euo pipefail

root_dir=$(cd "$(dirname "$0")/.." && pwd)

cd "$root_dir/app"
bun run tauri build --debug --no-bundle --features e2e

cd "$root_dir"
tools/build-worker.sh

cd "$root_dir/app"
bunx wdio run e2e/wdio.conf.mjs

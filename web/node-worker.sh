#!/bin/sh
# The wasm kernel as a drop-in worker: the host spawns this through
# PARCAD_OCCT_WORKER and talks the same stdin/stderr protocol to it, which is how
# the eval corpus measures the WebAssembly build unchanged.
#
#     PARCAD_OCCT_WORKER=$PWD/web/node-worker.sh cargo run -q --release -p parcad-eval
root=$(cd "$(dirname "$0")/.." && pwd)
exec node "${PARCAD_WASM_WORK:-$root/target/wasm}/node/parcad-occt-worker.js" "$@"

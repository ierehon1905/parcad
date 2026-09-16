#!/usr/bin/env bash
# Record the part the playground opens first, so a visitor sees it while the
# kernel is still downloading. `app/src/page/prebuilt.ts` says what the page
# does with this; in short, it stands in until the same graph has been built in
# the visitor's own tab, and the page says so on screen meanwhile.
#
#     playground/prebuild.sh                 # examples/twisted-planter.js
#     playground/prebuild.sh examples/bracket.js
#
# It writes target/playground/first-part.json — the evaluation as
# `/api/evaluate` answers it, with the mesh moved into first-part.drc by
# playground/encode-draco.ts — and the script beside it, which is what the page
# matches the open part against: the same text, unedited, or the kernel in the
# tab builds it. Neither is checked in: both are derived, and a stale one would
# describe a part the script no longer builds.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD

part=${1:-examples/twisted-planter.js}
out=$root/target/playground
port=${PARCAD_PREBUILD_PORT:-4386}
worker=${PARCAD_OCCT_WORKER:-$root/target/release/parcad-occt-worker}
parcad=${PARCAD_BIN:-$root/target/release/parcad}

for tool in "$parcad" "$worker"; do
  [ -x "$tool" ] || { echo "no $tool; build it with: cargo build --release --locked && tools/build-worker.sh" >&2; exit 1; }
done

mkdir -p "$out"
bun tools/run.ts "$part" > "$out/first-part-graph.json"
cp "$part" "$out/first-part-script.js"
basename "${part%.js}" > "$out/first-part-name.txt"

# Its own project folder: recording a part must not touch the user's, and the
# host seeds one on first run.
scratch=$(mktemp -d)
trap 'kill "${host:-0}" 2>/dev/null; rm -rf "$scratch"' EXIT
PARCAD_PROJECTS_DIR="$scratch/projects" PARCAD_SEED_DIR="$scratch/seed" PARCAD_OCCT_WORKER="$worker" \
  "$parcad" serve --port "$port" >"$scratch/host.log" 2>&1 &
host=$!

for _ in $(seq 1 60); do
  curl -sf "http://127.0.0.1:$port/api/projects" >/dev/null && break
  sleep 0.5
done

printf '{"graph": %s}' "$(cat "$out/first-part-graph.json")" |
  curl -sf -X POST "http://127.0.0.1:$port/api/evaluate" -H 'content-type: application/json' --data-binary @- \
    -o "$out/first-part.json"

bun playground/encode-draco.ts "$out/first-part.json"

python3 - "$out/first-part.json" <<'PY'
import json, sys
evaluated = json.load(open(sys.argv[1]))
snapshot = evaluated["snapshot"]
print(f"recorded {snapshot['triangles']} triangles, {snapshot['volume_mm3']} mm3, {snapshot['kernel_ms']} ms")
PY
ls -lh "$out/first-part.json" "$out/first-part.drc" | awk '{print "  " $5 "  " $9}'

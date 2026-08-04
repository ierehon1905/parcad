#!/usr/bin/env bash
# Time the B-rep kernel on a few parts.
#
# Reports the median of several runs per part, and separates the two costs that
# behave completely differently: `build` is OpenCASCADE doing booleans and
# fillets, `mesh` is tessellation. The floor — a 1 mm cube — is the fixed cost of
# spawning the worker process and moving JSON, which is what everything else is
# measured against.
#
#     tools/bench-kernel.sh <graph.json> [more.json ...]
set -euo pipefail
cd "$(dirname "$0")/.."

RUNS=${RUNS:-5}
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT

# Report what the kernel was compiled with, so a number is never quoted without
# knowing whether the thing producing it was optimised.
flags=$(find target/release/build/occt-sys-*/out/build -name flags.make -path "*TKBO*" 2>/dev/null | head -1)
if [ -n "$flags" ]; then
  opt=$(grep -o -- '-O[0123sz]' <<<"$(grep '^CXX_FLAGS =' "$flags")" | tail -1)
  echo "opencascade built with: ${opt:--O0 (UNOPTIMISED)}"
else
  echo "opencascade build flags: not found"
fi
echo

printf '%-22s %10s %10s %10s\n' part build_ms mesh_ms wall_ms
for graph in "$@"; do
  [ -f "$graph" ] || { echo "no such graph: $graph" >&2; continue; }

  builds=(); meshes=(); walls=()
  for _ in $(seq "$RUNS"); do
    line=$(./target/release/parcad "$graph" --brep --out "$OUT" 2>&1 | grep '^timing' || true)
    [ -z "$line" ] && continue
    builds+=("$(sed -E 's/.*build ([0-9]+) ms.*/\1/' <<<"$line")")
    meshes+=("$(sed -E 's/.*mesh ([0-9]+) ms.*/\1/' <<<"$line")")
    walls+=("$(sed -E 's/.*wall ([0-9]+) ms.*/\1/' <<<"$line")")
  done
  [ ${#builds[@]} -eq 0 ] && { printf '%-22s %10s\n' "$(basename "$graph" .json)" "REFUSED"; continue; }

  # Median, not mean: the first run of any part pays for page faults and a cold
  # cache, and one such outlier drags a mean of five well off the truth.
  med() { printf '%s\n' "$@" | sort -n | awk '{a[NR]=$1} END {print a[int((NR+1)/2)]}'; }
  printf '%-22s %10s %10s %10s\n' \
    "$(basename "$graph" .json)" \
    "$(med "${builds[@]}")" "$(med "${meshes[@]}")" "$(med "${walls[@]}")"
done

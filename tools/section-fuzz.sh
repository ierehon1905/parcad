#!/usr/bin/env bash
#
# Generate seeded section outlines and count where the DSL, the core and the
# kernel disagree about them. Not a gate: the gate is eval/sections.json.
#
#     tools/section-fuzz.sh [--seed N] [--per-family N] [--family NAME] [--keep DIR]
#
# Prints the outline count, each disagreement class with its message shapes
# and an example id, and the wall time. With --keep, DIR/outlines.jsonl and
# DIR/verdicts.jsonl stay behind for reading. docs/SECTION_CHECKS.md says what
# the classes mean and which are legitimate.
set -euo pipefail
cd "$(dirname "$0")/.."

keep=""
gen=()
while [ $# -gt 0 ]; do
  case "$1" in
    --keep) keep="$2"; shift 2 ;;
    --seed|--per-family|--family) gen+=("$1" "$2"); shift 2 ;;
    *) echo "usage: tools/section-fuzz.sh [--seed N] [--per-family N] [--family NAME] [--keep DIR]" >&2; exit 2 ;;
  esac
done

dir="${keep:-$(mktemp -d)}"
mkdir -p "$dir"
cargo build --locked --release -q -p parcad-occt --features kernel --example section_fuzz
started=$(date +%s)
bun tools/section-fuzz.ts ${gen[@]+"${gen[@]}"} > "$dir/outlines.jsonl"
target/release/examples/section_fuzz "$dir/outlines.jsonl" "$dir/verdicts.jsonl"
echo "total $(($(date +%s) - started)) s, generation included"
[ -n "$keep" ] || rm -rf "$dir"

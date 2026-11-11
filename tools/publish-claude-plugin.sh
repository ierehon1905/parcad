#!/usr/bin/env bash
#
# Publish packaging/claude-plugin to ierehon1905/parcad-claude-plugin, the small
# repository Claude's directory reads the plugin from. The directory refuses
# this one outright: its archive holds over 10,000 files and folders.
#
#     tools/publish-claude-plugin.sh
#
# The folder here stays the source: the tests in crates/parcad-cli/src/stdio.rs
# hold its version and its shared skill. Run this after a release bumps it.
set -euo pipefail
cd "$(dirname "$0")/.."

source_dir=packaging/claude-plugin
version=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' "$source_dir/.claude-plugin/plugin.json")
commit=$(git rev-parse --short HEAD)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
git clone -q git@github.com:ierehon1905/parcad-claude-plugin.git "$work/repo" 2>/dev/null
git -C "$work/repo" checkout -q -B main
rsync -a --delete --exclude .git "$source_dir/" "$work/repo/"

git -C "$work/repo" add -A
if git -C "$work/repo" diff --cached --quiet; then
  echo "parcad-claude-plugin already matches $source_dir ($version)"
  exit 0
fi
TZ=UTC git -C "$work/repo" commit -q -m "parcad $version, from ierehon1905/parcad@$commit"
git -C "$work/repo" push -q origin main
echo "published $source_dir $version to ierehon1905/parcad-claude-plugin"

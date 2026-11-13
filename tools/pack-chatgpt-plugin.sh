#!/usr/bin/env bash
#
# Pack packaging/chatgpt-plugin into the ZIP OpenAI's plugin portal takes:
# plugin.json at the archive's root, links followed, the README left out.
#
#     tools/pack-chatgpt-plugin.sh          # writes target/parcad-chatgpt-plugin-<version>.zip
set -euo pipefail
cd "$(dirname "$0")/.."

source_dir=packaging/chatgpt-plugin
version=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' "$source_dir/plugin.json")
out="$PWD/target/parcad-chatgpt-plugin-$version.zip"
mkdir -p target
rm -f "$out"
(cd "$source_dir" && zip -q -r -X "$out" plugin.json mcp.json skills assets)
unzip -l "$out"
echo "wrote $out"

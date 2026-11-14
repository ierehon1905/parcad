#!/usr/bin/env bash
#
# Pack packaging/chatgpt-plugin into the ZIP OpenAI's plugin portal takes:
# plugin.json at the archive's root, links followed, the README left out.
#
#     tools/pack-chatgpt-plugin.sh          # writes target/parcad-chatgpt-plugin-<version>.zip
#     DEMO_URL=https://youtu.be/... tools/pack-chatgpt-plugin.sh
#
# DEMO_URL replaces the review's demo_recording_url in the packed copy only: an
# unlisted recording for the reviewers stays out of the public repository.
set -euo pipefail
cd "$(dirname "$0")/.."

source_dir=packaging/chatgpt-plugin
version=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' "$source_dir/plugin.json")
out="$PWD/target/parcad-chatgpt-plugin-$version.zip"
mkdir -p target
rm -f "$out"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp -RL "$source_dir/plugin.json" "$source_dir/mcp.json" "$source_dir/skills" "$source_dir/assets" "$stage/"
if [ -n "${DEMO_URL:-}" ]; then
  python3 - "$stage/plugin.json" "$DEMO_URL" <<'EOF'
import json, sys
path, url = sys.argv[1], sys.argv[2]
d = json.load(open(path))
d["extensions"]["com.openai"]["review"]["demo_recording_url"] = url
json.dump(d, open(path, "w"), indent=2, ensure_ascii=False)
open(path, "a").write("\n")
EOF
fi
(cd "$stage" && zip -q -r -X "$out" plugin.json mcp.json skills assets)
unzip -l "$out"
echo "wrote $out"

#!/usr/bin/env bash
# Sign, notarise and staple the Mac app and CLI of a draft release, on this Mac,
# and put them back on the draft in place of CI's ad-hoc ones.
#
#     tools/sign-mac-release.sh v0.0.12              # between the tag's draft and publishing it
#     tools/sign-mac-release.sh --dry-run v0.0.11    # everything but the upload; any release
#
# CI never holds the Developer ID: release.yml drafts with an ad-hoc app, and this
# replaces the zip, the CLI archive and their lines in SHA256SUMS.txt, before
# publish.yml renders Homebrew from those checksums. The updater archive and
# latest.json are replaced too when the updater key is here: TAURI_SIGNING_PRIVATE_KEY,
# or ~/.tauri/parcad-updater.key and .password (packaging/README.md, "The updater").
set -euo pipefail
cd "$(dirname "$0")/.."

dry=0
if [ "${1:-}" = --dry-run ]; then dry=1; shift; fi
[ $# = 1 ] || { echo "usage: tools/sign-mac-release.sh [--dry-run] v<version>" >&2; exit 2; }
tag="$1"
triple=aarch64-apple-darwin
zip="ParCAD-$triple.zip"
updater="ParCAD-$triple.app.tar.gz"
cli="parcad-cli-$triple"

draft=$(gh release view "$tag" --json isDraft -q .isDraft) \
  || { echo "no release $tag; push the tag and wait for release.yml's draft" >&2; exit 1; }
if [ "$draft" != true ] && [ "$dry" = 0 ]; then
  echo "$tag is published: Homebrew and winget already carry its checksums. Sign the next draft, or --dry-run" >&2
  exit 1
fi

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && [ -f ~/.tauri/parcad-updater.key ]; then
  export TAURI_SIGNING_PRIVATE_KEY=~/.tauri/parcad-updater.key
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD=$(cat ~/.tauri/parcad-updater.password 2>/dev/null || true)
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
gh release download "$tag" -D "$work/in" -p "$zip" -p "$cli.tar.gz" -p "$updater" \
  -p latest.json -p SHA256SUMS.txt
mkdir "$work/app" "$work/out"
ditto -x -k "$work/in/$zip" "$work/app"
tar xzf "$work/in/$cli.tar.gz" -C "$work/app"

tools/sign-mac.sh "$work/app/ParCAD.app" "$work/app/$cli/parcad" "$work/app/$cli/parcad-occt-worker"

# Packed exactly as release.yml's "Collect the bundles" and "Stage the headless CLI" do.
ditto -c -k --keepParent "$work/app/ParCAD.app" "$work/out/$zip"
tar czf "$work/out/$cli.tar.gz" -C "$work/app" "$cli"
replaced=("$zip" "$cli.tar.gz")

if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  COPYFILE_DISABLE=1 tar --no-mac-metadata -czf "$work/out/$updater" -C "$work/app" ParCAD.app
  (cd app && bun run tauri signer sign "$work/out/$updater" > /dev/null)
  sig=$(cat "$work/out/$updater.sig")
  jq --arg sig "$sig" '.platforms |= with_entries(if .key | startswith("darwin-") then .value.signature = $sig else . end)' \
    "$work/in/latest.json" > "$work/out/latest.json"
  replaced+=("$updater" "$updater.sig" latest.json)
else
  echo "no updater key here: $updater stays ad-hoc, which an update installs without Gatekeeper asking"
fi

cp "$work/in/SHA256SUMS.txt" "$work/out/SHA256SUMS.txt"
for f in "${replaced[@]}"; do
  line=$(cd "$work/out" && shasum -a 256 "$f")
  grep -q "  $f\$" "$work/out/SHA256SUMS.txt" \
    && sed -i '' "s|^[0-9a-f]*  $f\$|$line|" "$work/out/SHA256SUMS.txt" \
    || echo "$line" >> "$work/out/SHA256SUMS.txt"
done
replaced+=(SHA256SUMS.txt)

if [ "$dry" = 1 ]; then
  echo "dry run:          would replace on $tag: ${replaced[*]}"
  exit 0
fi

(cd "$work/out" && gh release upload "$tag" --clobber "${replaced[@]}")

# Read back what a user would download, quarantined as a browser would leave it.
mkdir "$work/check"
gh release download "$tag" -D "$work/check" -p "$zip"
xattr -w com.apple.quarantine "0081;$(printf %x "$(date +%s)");Safari;" "$work/check/$zip"
ditto -x -k "$work/check/$zip" "$work/check"
spctl -a -t exec "$work/check/ParCAD.app"
echo "uploaded:         ${replaced[*]}"
echo "                  $tag's $zip opens quarantined: accepted, notarised"

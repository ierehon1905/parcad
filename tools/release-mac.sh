#!/usr/bin/env bash
# Build ParCAD.app and its DMG signed with a Developer ID, notarised and stapled.
#
#     tools/release-mac.sh                  # sign, notarise, staple
#     tools/release-mac.sh --no-notarize    # sign only: opens here, not on another Mac
#
# PARCAD_SIGNING_IDENTITY picks the certificate; unset, the keychain's only
# "Developer ID Application" identity is used. PARCAD_NOTARY_PROFILE names the
# notarytool keychain profile, parcad-notary by default; create it once with
#
#     xcrun notarytool store-credentials parcad-notary --apple-id <email> --team-id <team>
#
# tauri.conf.json stays ad-hoc for CI; the identity is a --config override here.
set -euo pipefail
cd "$(dirname "$0")/.."

notarize=1
case "${1:-}" in
  --no-notarize) notarize=0 ;;
  "") ;;
  *) echo "usage: tools/release-mac.sh [--no-notarize]" >&2; exit 2 ;;
esac

[ "$(uname -s)" = Darwin ] || { echo "a Developer ID signature needs macOS and its keychain" >&2; exit 1; }

identity="${PARCAD_SIGNING_IDENTITY:-}"
if [ -z "$identity" ]; then
  found=$(security find-identity -v -p codesigning | grep -o '"Developer ID Application: [^"]*"' | tr -d '"' | sort -u)
  case "$(printf '%s' "$found" | grep -c .)" in
    1) identity="$found" ;;
    0) echo "no Developer ID Application certificate in the keychain; install one from developer.apple.com, Certificates" >&2; exit 1 ;;
    *) echo "several Developer ID Application certificates; pick one with PARCAD_SIGNING_IDENTITY:" >&2
       printf '  %s\n' "$found" >&2; exit 1 ;;
  esac
fi
echo "signing as:       $identity"

profile="${PARCAD_NOTARY_PROFILE:-parcad-notary}"
# Checked before a ten-minute build rather than after it.
if [ "$notarize" = 1 ] && ! xcrun notarytool history --keychain-profile "$profile" >/dev/null 2>&1; then
  echo "no notarytool credentials under '$profile'; store them with" >&2
  echo "  xcrun notarytool store-credentials $profile --apple-id <email> --team-id <team>" >&2
  echo "or build without notarising: tools/release-mac.sh --no-notarize" >&2
  exit 1
fi

tools/build-worker.sh --release

config=$(jq -nc --arg id "$identity" \
  '{build: {beforeBuildCommand: "bun run build"}, bundle: {macOS: {signingIdentity: $id, hardenedRuntime: true}}}')

(cd app && bun install --frozen-lockfile && bun run tauri build --no-bundle --config "$config" \
  -- -p parcad-app -p parcad-cli --locked)

# The bundler signs the app and its externalBin, but copies `files` as they are:
# MacOS/parcad would ship linker-signed, and notarisation rejects the whole app
# for it. Signed here, after cargo's last link, since cargo relinks it on build.
codesign --force --options runtime --timestamp --sign "$identity" target/release/parcad

# `tauri bundle` packages what `tauri build` compiled without compiling again.
(cd app && bun run tauri bundle --bundles app,dmg --config "$config")

version=$(jq -r .version app/src-tauri/tauri.conf.json)
app=target/release/bundle/macos/ParCAD.app
dmg=$(ls target/release/bundle/dmg/ParCAD_"$version"_*.dmg)

for bin in "$app"/Contents/MacOS/*; do
  details=$(codesign -dvv "$bin" 2>&1)
  grep -q "^Authority=$identity\$" <<<"$details" && grep -q 'flags=.*(runtime)' <<<"$details" \
    || { echo "$bin is not signed by $identity with the hardened runtime; notarisation would refuse it" >&2; exit 1; }
done
codesign --verify --deep --strict "$app"
codesign --verify "$dmg"
echo "signed:           $app and $dmg"

if [ "$notarize" = 0 ]; then
  echo "not notarised:    Gatekeeper on another Mac will refuse it"
  exit 0
fi

# One submission covers the DMG and everything in it, the app included.
out=$(xcrun notarytool submit "$dmg" --keychain-profile "$profile" --wait --output-format json)
id=$(jq -r .id <<<"$out")
if [ "$(jq -r .status <<<"$out")" != Accepted ]; then
  echo "notarisation returned $(jq -r .status <<<"$out"); Apple's reasons:" >&2
  xcrun notarytool log "$id" --keychain-profile "$profile" >&2
  exit 1
fi
echo "notarised:        submission $id"

xcrun stapler staple -q "$dmg"
xcrun stapler staple -q "$app"
spctl -a -t open --context context:primary-signature "$dmg"
spctl -a -t exec "$app"
echo "stapled:          $dmg"
echo "                  $app"

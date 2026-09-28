#!/usr/bin/env bash
# Sign an already-bundled ParCAD.app, and any loose binaries, with a Developer ID
# and the hardened runtime, notarise them in one submission and staple the app.
#
#     tools/sign-mac.sh ParCAD.app [parcad parcad-occt-worker ...]
#     tools/sign-mac.sh --no-notarize ParCAD.app
#
# The Developer ID stays in this Mac's keychain and never reaches CI;
# tools/sign-mac-release.sh runs this on a draft release's ad-hoc bundle.
# PARCAD_SIGNING_IDENTITY and PARCAD_NOTARY_PROFILE are as in tools/release-mac.sh.
# A loose binary cannot be stapled: Gatekeeper finds its ticket online.
set -euo pipefail

notarize=1
if [ "${1:-}" = --no-notarize ]; then notarize=0; shift; fi
[ $# -ge 1 ] && [ -d "$1" ] && [ "${1%.app}" != "$1" ] \
  || { echo "usage: tools/sign-mac.sh [--no-notarize] <the .app> [binary ...]" >&2; exit 2; }
app="${1%/}"; shift
binaries=("$@")

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
notary=(--keychain-profile "$profile")
if [ "$notarize" = 1 ] && ! xcrun notarytool history "${notary[@]}" >/dev/null 2>&1; then
  echo "no notarytool credentials under '$profile'; store them with" >&2
  echo "  xcrun notarytool store-credentials $profile --apple-id <email> --team-id <team>" >&2
  echo "or sign without notarising: tools/sign-mac.sh --no-notarize ..." >&2
  exit 1
fi

sign() { codesign --force --options runtime --timestamp --sign "$identity" "$@"; }

# Inside out: every Mach-O in the bundle before the bundle, whose seal covers them.
while IFS= read -r -d '' f; do
  if file -b "$f" | grep -q '^Mach-O'; then sign "$f"; fi
done < <(find "$app/Contents" -type f -perm -u+x -print0)
sign "$app"
for bin in "${binaries[@]}"; do sign "$bin"; done

for bin in "$app"/Contents/MacOS/* "${binaries[@]}"; do
  details=$(codesign -dvv "$bin" 2>&1)
  grep -q "^Authority=$identity\$" <<<"$details" && grep -q 'flags=.*(runtime)' <<<"$details" \
    || { echo "$bin is not signed by $identity with the hardened runtime; notarisation would refuse it" >&2; exit 1; }
done
codesign --verify --deep --strict "$app"
echo "signed:           $app ${binaries[*]}"

if [ "$notarize" = 0 ]; then
  echo "not notarised:    Gatekeeper on another Mac will refuse it"
  exit 0
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/submit"
ditto "$app" "$stage/submit/$(basename "$app")"
for bin in "${binaries[@]}"; do ditto "$bin" "$stage/submit/$(basename "$bin")"; done
ditto -c -k "$stage/submit" "$stage/submit.zip"

out=$(xcrun notarytool submit "$stage/submit.zip" "${notary[@]}" --wait --output-format json)
id=$(jq -r .id <<<"$out")
if [ "$(jq -r .status <<<"$out")" != Accepted ]; then
  echo "notarisation returned $(jq -r .status <<<"$out"); Apple's reasons:" >&2
  xcrun notarytool log "$id" "${notary[@]}" >&2
  exit 1
fi
echo "notarised:        submission $id"

xcrun stapler staple -q "$app"
spctl -a -t exec "$app"
echo "stapled:          $app"

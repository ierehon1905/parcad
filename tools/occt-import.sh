#!/usr/bin/env bash
# Re-export the vendored OpenCASCADE tree from an upstream tag.
#
# `vendor/occt-sys/OCCT` is committed rather than carried as a git submodule, so
# that `git clone && cargo build` works offline with no second step — the same
# reason Cargo.lock and bun.lock are committed. This script is what keeps that
# honest: it re-derives the tree from upstream, so the claim that it is pristine
# is checkable rather than promised.
#
#   tools/occt-import.sh              # verify the tree matches its recorded tag
#   tools/occt-import.sh V8_0_2       # upgrade to a new tag
#
# Our own changes are not in that tree — they are diffs in
# vendor/occt-sys/patches, applied at build time. After an upgrade, run the
# build: any patch that no longer applies fails and names itself.
set -euo pipefail
cd "$(dirname "$0")/.."

# The tag this tree was exported from. Bump it here and in PARCAD-CHANGES.md.
RECORDED_TAG="V8_0_1"
TAG="${1:-$RECORDED_TAG}"
DEST="vendor/occt-sys/OCCT"

# Not source, and none of it reaches the build: 145 MB of test corpus, sample
# data and doxygen. Everything under src/ is kept, including modules we do not
# compile, so the tree stays byte-identical to upstream where it matters.
PRUNE=(tests data dox samples .github)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

url="https://github.com/Open-Cascade-SAS/OCCT/archive/refs/tags/${TAG}.tar.gz"
echo "fetching $url"
curl -fsSL "$url" -o "$work/occt.tar.gz"
tar xzf "$work/occt.tar.gz" -C "$work"
src="$(find "$work" -maxdepth 1 -type d -name 'OCCT-*' | head -1)"
[ -n "$src" ] || { echo "no OCCT-* directory in the tag archive" >&2; exit 1; }

for p in "${PRUNE[@]}"; do rm -rf "${src:?}/$p"; done

if [ "$TAG" = "$RECORDED_TAG" ] && [ -d "$DEST" ]; then
  if diff -rq "$src" "$DEST" > "$work/drift" 2>&1; then
    echo "ok: $DEST is a pristine export of $TAG"
    exit 0
  fi
  echo "DRIFT: $DEST does not match upstream $TAG" >&2
  sed 's/^/  /' "$work/drift" >&2
  echo >&2
  echo "  Someone edited the vendored tree in place. Changes to OpenCASCADE" >&2
  echo "  belong in vendor/occt-sys/patches — see the README there." >&2
  echo "  Fix: move the change into a patch, then re-run this script with the" >&2
  echo "  tag name to restore the tree." >&2
  exit 1
fi

echo "replacing $DEST with $TAG"
rm -rf "$DEST"
mv "$src" "$DEST"
echo "done. Now:"
echo "  1. set RECORDED_TAG=$TAG in this script"
echo "  2. record the upgrade in vendor/occt-sys/PARCAD-CHANGES.md"
echo "  3. tools/check.sh — a patch that no longer applies will name itself"

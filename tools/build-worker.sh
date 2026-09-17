#!/usr/bin/env bash
# Build the B-rep kernel worker and put it where the host will look for it.
#
#     tools/build-worker.sh             # the iterate profile: the local loop
#     tools/build-worker.sh --release   # what the corpus, benchmarks and bundles run
#
# The worker is a separate process on purpose — see crates/parcad-occt/src/host.rs
# — and always optimised: OCCT behind a debug-profile Rust half is slow enough
# to feel broken, and this binary contains no code we step through. Both
# profiles share release's opt-level; iterate drops the LTO and single codegen
# unit that make a release relink slow (see Cargo.toml).
#
# A worker serves only a host of its own sources, and a release worker only a
# release host (protocol::BuildId), so each profile's worker is installed only
# beside the hosts it can serve: iterate beside target/iterate and target/debug,
# release beside target/release and into the bundle's sidecar.
#
# `cargo build -p parcad-occt` alone is not enough: without --features kernel the
# crate is just the host half and the binary is skipped entirely.
set -euo pipefail
cd "$(dirname "$0")/.."

profile=iterate
case "${1:-}" in
  --release) profile=release ;;
  "") ;;
  *) echo "usage: tools/build-worker.sh [--release]" >&2; exit 2 ;;
esac

# One OpenCASCADE for both profiles. Cargo builds a build-dependency once per
# profile directory, so iterate would otherwise compile OCCT a second time for
# byte-identical inputs; cargo names the release install it has just brought up
# to date, and occt-sys relinks when that install changes.
if [ "$profile" = iterate ] && [ -z "${PARCAD_OCCT_PREBUILT:-}" ]; then
  PARCAD_OCCT_PREBUILT=$(cargo build --locked --release -p opencascade-sys --message-format=json |
    jq -r 'select(.reason == "build-script-executed" and (.package_id | test("occt-sys"))) | .out_dir')
  [ -d "$PARCAD_OCCT_PREBUILT/include" ] || { echo "cargo named no OpenCASCADE install for the release profile" >&2; exit 1; }
  export PARCAD_OCCT_PREBUILT
  echo "opencascade:      shared with the release profile ($PARCAD_OCCT_PREBUILT)"
fi

cargo build --locked -p parcad-occt --features kernel --profile "$profile" --bins

triple=$(rustc -vV | awk '/^host: /{print $2}')
exe=""
case "$triple" in *windows*) exe=".exe" ;; esac
worker="target/$profile/parcad-occt-worker$exe"
echo "worker built:     $worker ($(ls -la "$worker" | awk '{print $5" bytes"}'))"

if [ "$profile" = iterate ]; then
  # Beside the dev-profile hosts too: `tauri dev`, `cargo run`, `cargo test`.
  mkdir -p target/debug
  cp "$worker" target/debug/
  echo "worker installed: target/debug/parcad-occt-worker$exe"
else
  # Where `tauri build` looks, which is not the same place or the same name. An
  # `externalBin` entry names the file without its target triple and the bundler
  # demands the file on disk carry one. It is gitignored.
  mkdir -p app/src-tauri/binaries
  cp "$worker" "app/src-tauri/binaries/parcad-occt-worker-$triple$exe"
  echo "sidecar staged:   app/src-tauri/binaries/parcad-occt-worker-$triple$exe"
fi

# Check OpenCASCADE actually got optimised.
#
# The `cmake` crate configures OCCT as a Debug build even under --release, so
# without the CXXFLAGS in .cargo/config.toml the entire geometry kernel compiles
# at -O0 and every boolean and fillet runs several times slower than it should.
# That is invisible — everything works, it is just slow — so it is worth an
# explicit check rather than trusting the config to have been read.
# A PARCAD_OCCT_PREBUILT install was compiled elsewhere; its flags live under
# its own build tree, not this checkout's target/.
flag_root="${PARCAD_OCCT_PREBUILT:+$PARCAD_OCCT_PREBUILT/build}"
flags=$(find ${flag_root:-target/$profile/build/occt-sys-*/out/build} -name flags.make -path "*TKBO*" 2>/dev/null | head -1 || true)
if [ -z "$flags" ]; then
  # A Visual Studio build writes .vcxproj files, not flags.make.
  echo "opencascade:      optimisation not checked (no flags.make under the OCCT build)"
else
  if grep -q '^CXX_FLAGS =.*-O[123s]' "$flags"; then
    echo "opencascade:      optimised ($(grep -o -- '-O[123s]' "$flags" | head -1))"
  else
    echo
    echo "WARNING: OpenCASCADE was built WITHOUT optimisation." >&2
    echo "  Booleans and fillets will run several times slower than they should." >&2
    echo "  Cause: CXXFLAGS set in your environment overrides .cargo/config.toml." >&2
    echo "  Fix:   unset CXXFLAGS CFLAGS && rm -rf target/*/build/occt-sys-* && $0" >&2
    echo "  Flags: $(grep '^CXX_FLAGS =' "$flags")" >&2
  fi
fi

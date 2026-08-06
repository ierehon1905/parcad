#!/usr/bin/env bash
# Build the B-rep kernel worker and put it where the host will look for it.
#
# The worker is a separate process on purpose — see crates/parcad-occt/src/host.rs
# — and always release-built: OCCT in a debug profile is slow enough to feel
# broken, and this binary contains no code we step through.
#
# `cargo build -p parcad-occt` alone is not enough: without --features kernel the
# crate is just the host half and the binary is skipped entirely.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --locked -p parcad-occt --features kernel --release --bins

# Beside every application binary, which is where host::worker_path looks.
for dir in target/debug target/release; do
  [ -d "$dir" ] && cp target/release/parcad-occt-worker "$dir/" 2>/dev/null || true
done
echo "worker installed: $(ls -la target/release/parcad-occt-worker | awk '{print $5" bytes"}')"

# And where Tauri looks, which is not the same place or the same name. An
# `externalBin` entry names the file without its target triple and the bundler
# demands the file on disk carry one, so this copy exists purely to satisfy
# `tauri build`. It is a build artifact of the size of a geometry kernel and is
# gitignored; without it the bundle silently ships no worker.
triple=$(rustc -vV | awk '/^host: /{print $2}')
mkdir -p app/src-tauri/binaries
cp target/release/parcad-occt-worker "app/src-tauri/binaries/parcad-occt-worker-$triple"
echo "sidecar staged:   app/src-tauri/binaries/parcad-occt-worker-$triple"

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
flags=$(find ${flag_root:-target/release/build/occt-sys-*/out/build} -name flags.make -path "*TKBO*" 2>/dev/null | head -1)
if [ -n "$flags" ]; then
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

#!/usr/bin/env bash
# Build the exact kernel for WebAssembly: the vendored, patched OpenCASCADE and
# the Rust worker around it, compiled by Emscripten.
#
#     EMSDK=/path/to/emsdk playground/build-kernel.sh           # everything
#     EMSDK=/path/to/emsdk playground/build-kernel.sh occt      # OpenCASCADE only
#     EMSDK=/path/to/emsdk playground/build-kernel.sh worker    # the Rust half only
#
# This is also the relinking recipe NOTICE.md promises: point OCCT_SOURCE at
# another OpenCASCADE tree and the same commands produce a kernel built on it.
# playground/README.md has the pinned versions and what each output is for.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD

: "${EMSDK:?set EMSDK to an activated emsdk checkout (playground/README.md pins the version)}"
# shellcheck disable=SC1091
source "$EMSDK/emsdk_env.sh" >/dev/null 2>&1

work=${PARCAD_WASM_WORK:-$root/target/wasm}
occt_source=${OCCT_SOURCE:-$root/vendor/occt-sys/OCCT}
staged=$work/occt-src
build=$work/occt-build
install=$work/occt
what=${1:-all}

started=$(date +%s)
step() { printf '\n== %s (%ss)\n' "$1" "$(($(date +%s) - started))"; }

build_occt() {
  step "staging OpenCASCADE from $occt_source"
  mkdir -p "$staged"
  # The same "pristine tree plus a patch series" vendor/occt-sys/build.rs
  # applies. By content and with fresh mtimes: a file that is rewritten must
  # look newer to ninja, which would otherwise keep an object compiled from
  # whatever was staged before (measured: it did); an unchanged file is left
  # alone and recompiles nothing.
  rsync -a --no-times --checksum --delete "$occt_source/" "$staged/"
  for patch in "$root"/vendor/occt-sys/patches/*.patch; do
    echo "applying $(basename "$patch")"
    patch -d "$staged" -p1 --forward --silent < "$patch"
  done

  step "configuring OpenCASCADE for wasm32"
  # The same switches vendor/occt-sys/build.rs passes, so the wasm kernel is the
  # native one compiled by a different compiler.
  CMAKE_POLICY_VERSION_MINIMUM=3.5 emcmake cmake -G Ninja -S "$staged" -B "$build" \
    -DCMAKE_TOOLCHAIN_FILE="$root/playground/occt-emscripten.cmake" \
    -DCMAKE_BUILD_TYPE=Debug \
    -DBUILD_LIBRARY_TYPE=Static \
    -DBUILD_MODULE_Draw=FALSE \
    -DBUILD_INCLUDE_SYMLINK=ON \
    -DUSE_D3D=FALSE -DUSE_DRACO=FALSE -DUSE_EIGEN=FALSE -DUSE_FFMPEG=FALSE \
    -DUSE_FREEIMAGE=FALSE -DUSE_FREETYPE=FALSE -DUSE_GLES2=FALSE -DUSE_OPENGL=FALSE \
    -DUSE_OPENVR=FALSE -DUSE_RAPIDJSON=FALSE -DUSE_TBB=FALSE -DUSE_TCL=FALSE \
    -DUSE_TK=FALSE -DUSE_VTK=FALSE -DUSE_XLIB=FALSE \
    >"$work/occt-configure.log"

  # Only the toolkits the worker links, read from the file that links them.
  toolkits=$(grep -o 'static=TK[A-Za-z0-9]*' vendor/opencascade-sys/build.rs | sed 's/static=//' | sort -u | tr '\n' ' ')
  step "compiling $toolkits"
  # shellcheck disable=SC2086
  ninja -C "$build" $toolkits

  step "collecting the install"
  rm -rf "$install"
  mkdir -p "$install/lib"
  find "$build" -name 'libTK*.a' -exec cp {} "$install/lib/" \;
  # Flat, as INSTALL_DIR_INCLUDE=include lays out the native install.
  cp -RL "$build/include/opencascade" "$install/include"
  # Without symlinks OCCT writes one-line headers that #include the staged source
  # by absolute path, an install that breaks once cached without occt-src.
  if grep -rlq '^#include "/' "$install/include"; then
    echo "opencascade: $install/include forwards into $staged; the install is not self-contained" >&2
    exit 1
  fi
  if grep -q -- '-O2' "$build/build.ninja" 2>/dev/null || grep -rq -- '-O2' "$build/CMakeFiles/rules.ninja" 2>/dev/null; then
    echo "opencascade: optimised (-O2)"
  else
    echo "WARNING: OpenCASCADE flags carry no -O2; see cmake/occt-toolchain.cmake" >&2
  fi
  du -sh "$install/lib" "$install/include"

  # rustc copies every static OpenCASCADE library into opencascade-sys's rlib
  # when it compiles that crate, and nothing tells cargo the .a files changed:
  # without this, a rebuilt OCCT links the previous one. See docs/GOTCHAS.md.
  CARGO_TARGET_DIR="$work/cargo" cargo clean --release --target wasm32-unknown-emscripten -p opencascade-sys
}

build_worker() {
  step "compiling the worker and the browser kernel for wasm32-unknown-emscripten"
  [ -d "$install/lib" ] || { echo "no OpenCASCADE at $install; run: $0 occt" >&2; exit 1; }
  # A separate target directory: occt-sys is a build-dependency compiled for the
  # host, and PARCAD_OCCT_PREBUILT here names a wasm install, which a native
  # build in target/ must never pick up. Link settings live in each crate's
  # build.rs: crates/parcad-occt for the worker under Node, crates/parcad-wasm
  # for the module a browser loads.
  PARCAD_OCCT_PREBUILT="$install" \
    CXXFLAGS_wasm32_unknown_emscripten="-fwasm-exceptions -O2" \
    CARGO_TARGET_DIR="$work/cargo" \
    cargo build --locked --release --target wasm32-unknown-emscripten \
      -p parcad-occt --features parcad-occt/kernel --bin parcad-occt-worker \
      -p parcad-wasm --bin parcad-wasm
  out=$work/cargo/wasm32-unknown-emscripten/release
  mkdir -p "$work/node" "$work/web"
  cp "$out/parcad-occt-worker.js" "$out/parcad_occt_worker.wasm" "$work/node/"
  [ -f "$out/parcad-wasm.js" ] && cp "$out/parcad-wasm.js" "$out/parcad_wasm.wasm" "$work/web/"
  ls -la "$work/node" "$work/web"
}

case "$what" in
  occt) build_occt ;;
  worker) build_worker ;;
  all) build_occt; build_worker ;;
  *) echo "usage: $0 [occt|worker|all]" >&2; exit 2 ;;
esac
step "done"

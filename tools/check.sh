#!/usr/bin/env bash
#
# Everything that has to be true before a change lands, in the one order that
# works. Run it bare for the full pass, or `--fast` to skip the corpus.
#
#     tools/check.sh              # build, unit tests, worker, eval corpus
#     tools/check.sh --fast       # everything but the corpus (~20 s)
#
# The order is not arbitrary:
#
#   * The worker is built *after* `cargo test`. Testing `parcad-occt` without
#     `--features kernel` builds the crate without its binary target, and cargo
#     then deletes the `parcad-occt-worker` it finds next to it — every case in
#     the corpus turns to SKIP, which reads exactly like a pass. See the gotcha
#     of the same name in docs/GOTCHAS.md.
#   * The corpus runs last because it is the only step that needs the worker,
#     and it finds it beside its own executable in target/release.
#   * `field/selftest.py` is in both paths and costs nothing: it runs no model
#     and touches no network, it only regrades recorded transcripts.
set -euo pipefail
cd "$(dirname "$0")/.."

fast=0
case "${1:-}" in
  --fast) fast=1 ;;
  "") ;;
  *) echo "usage: tools/check.sh [--fast]" >&2; exit 2 ;;
esac

# Each step announces how long the run has been going. The release build and the
# worker relink each emit nothing for over a minute, so an elapsed count is the
# only thing distinguishing "still linking" from "wedged".
started=$(date +%s)
step() { printf '\n\033[1m== %s\033[0m \033[2m(%ss)\033[0m\n' "$1" "$(($(date +%s) - started))"; }

# A clone has no app/node_modules, and the editor-side tests import CodeMirror.
# Without this the first gate a contributor runs dies inside bun's resolver.
need_frontend_deps() {
  [ -d app/node_modules ] || {
    echo "app/node_modules is missing. Run: (cd app && bun install --frozen-lockfile)" >&2
    exit 1
  }
}

if [ "$fast" = 1 ]; then
  # The kernel crates only. `parcad-app` is left out on purpose: its test binary
  # is slow to link and one of its tests sleeps 5 s by design (the sandbox's
  # endless-script timeout), which together are the whole difference between a
  # 0.3 s loop and a 6 s one. Nothing here reaches the app, so the app is not
  # what a fast loop is checking.
  # One cargo invocation, not a build followed by a test: `cargo test` builds
  # every lib and bin it needs anyway, and a separate `cargo build` only adds a
  # second link of the same crates.
  # Not --release: that profile is LTO'd and non-incremental on purpose, which
  # is right for the thing that runs and wrong for the thing you run every few
  # minutes. The `test` profile is incremental at opt-level 2 (see Cargo.toml),
  # so the tests still execute in hundredths of a second.
  step "cargo test -p parcad-core -p parcad-occt -p parcad-cli"
  cargo test --locked -p parcad-core -p parcad-occt -p parcad-cli

  step "bun test"
  need_frontend_deps
  (cd app && bun test src)

  step "field/selftest.py"
  field/selftest.py

  # No worker here. Building it recompiles parcad-occt under `--features kernel`
  # and relinks 26 MB of statically-bound OpenCASCADE, which is most of a
  # kernel-edit iteration — and nothing in this path runs geometry. The corpus
  # needs it; the corpus is not in this path.
  printf '\n\033[1mok\033[0m — kernel only, no worker, no corpus. Run tools/check.sh before pushing.\n'
  exit 0
fi

step "cargo build --locked --release"
cargo build --locked --release

step "cargo test"
# `occt-sys` is excluded because it only reaches the test build as a selected
# member, and a member is built for the target with the release profile —
# a second unit for its build script, whose run *is* the OpenCASCADE compile.
# Everything else reaches it as a build-dependency, which the worker build
# already made. Selected, it compiled OCCT twice per cold checkout: 7 GB and
# five minutes for a crate with no tests. See docs/GOTCHAS.md.
cargo test --release --workspace --exclude occt-sys

step "bun test"
need_frontend_deps
(cd app && bun test src)

step "field/selftest.py"
field/selftest.py

step "tools/build-worker.sh"
tools/build-worker.sh

step "eval corpus"
cargo run -q -p parcad-eval

printf '\n\033[1mall green\033[0m\n'

# Contributing to ParCAD

ParCAD is early — version 0.0.1, one author so far, and the DSL is still moving.
Issues, questions and patches are all welcome, including "I tried to model X and
gave up here", which is the most useful bug report this project gets:
[docs/DSL_GAPS.md](docs/DSL_GAPS.md) was written from exactly that.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), then
[docs/GOTCHAS.md](docs/GOTCHAS.md). Most of what looks like a bug here has
already been diagnosed once.

## Setting up

Prerequisites, beyond the ones in [README.md](README.md#build-it): Python 3.11 or
newer (the grader's self-test uses `tomllib`), `patch(1)` on `PATH` (OCCT is
built from vendored sources plus patches), and about 20 GB free — the checked-out
OCCT tree is 144 MB and a full `target/` reaches the tens of gigabytes.

```bash
cd app && bun install --frozen-lockfile && cd ..   # before anything else: cargo build shells out to bun
git config core.hooksPath .githooks                # the gate; a clone does not get this automatically
export TZ=UTC                                      # see below
```

Development is done on macOS. Linux should work and is untested — patches
welcome. Windows is not supported: the B-rep worker's process handling is
`#[cfg(unix)]` with no Windows arm.

**Commits are recorded in UTC.** A commit's timezone offset is a statement about
where its author was sitting, git takes it from the environment, and there is no
config for it — so `.githooks/pre-commit` refuses a commit made under any other
offset rather than silently recording one. `TZ=UTC git commit ...`, or export it
for the session.

## The gate

```bash
tools/check.sh          # build, unit tests, the B-rep worker, the eval corpus (~5 min)
tools/check.sh --fast   # kernel crates, editor tests, grader self-test (~4 s)
```

`pre-commit` runs `--fast`, `pre-push` runs the whole thing. `--fast` builds no
worker and runs no geometry, so it is the edit loop, not the claim. Run the full
pass before you push, and never reach for `--release` to make an edit-test loop
faster — that profile is LTO'd and non-incremental on purpose.

## What a change has to satisfy

**Never claim geometry is correct without measuring it.** OCCT returns
valid-looking wrong answers routinely; check volume, bounding box, or face count.
The `-O0` bug in docs/GOTCHAS.md survived because the Rust layer was verified and
the C++ under it was assumed. `eval/cases/` makes a measurement permanent: add a
case with each new operation, and `cargo run -p parcad-eval -- --update` when a
change to a recorded number is intended — read that diff, it is the whole point.
A case that is green on a wrong recorded number is worse than no case; that is
what `known_defect` is for.

**And never claim an agent-facing tool works because its output is correct.**
Whether a model *reads* that output is a separate fact, measured separately by
`field/run-suite.sh`, and it has been wrong every time it was checked. A
capability only a person can reach does not compound — it has to be on the agent
surface too, with a description a model that has never seen this repository can
act on.

**Refuse rather than approximate.** Non-uniform scale, blended intersection and
general offsets all `bail!` with a reason. Don't add a "close enough" path.

**Report measured values, not requested ones**, and **make error messages name
the fix** — an agent-facing tool whose errors only say what failed is half-built.

## Traps worth knowing before you start

- **A new op touches six places**: `graph.rs` (variant *and* `children_of`),
  `sdf.rs`, `measure.rs`, `backend.rs`, `app/src/dsl.ts`, plus a case in
  `eval/cases/`. A missing `children_of` arm is silent — the node simply never
  gets evaluated. Rust's exhaustive matches will find the rest for you.
- **The selector grammar is parsed twice on purpose**, in `selectors.rs` and
  `app/src/selectors.ts`. Neither copy is the specification —
  `eval/selectors.json` is, and both languages' tests run against it. Change a
  rule in one language only and a test goes red rather than the editor quietly
  accepting what the kernel later refuses.
- **Every DSL export becomes a reserved word in a saved part.** Parts run as
  `new Function(...names, source)`, so a new export called `hole` breaks every
  part that wrote `const hole = ...`. Adding one is a compatibility change.
- **Units are millimetres, always**, and primitives are centred on the origin,
  placed with a separate `Translate`.
- **`cargo build` produces a *dev* app whatever the profile.** Tauri's switch is
  the `custom-protocol` feature its CLI adds, not `--release`.
- **The product is ParCAD; everything a machine reads is `parcad`.** The bundle
  id, the project folder and the binary names are load-bearing: renaming them
  orphans parts users have already saved.

[CLAUDE.md](CLAUDE.md) carries the full conventions, including the frontend rules
and a "where things live" table mapping every kind of change to its file. It is
addressed to a coding agent, but the facts in it are the same ones a human needs.

## Vendored code

`vendor/opencascade` is a fork of the crates.io crate, kept minimal so it can go
upstream, and `vendor/occt-sys` carries OpenCASCADE itself. **Every change to
either goes in that directory's `PARCAD-CHANGES.md`.** Both are LGPL-2.1 while
our own crates are MIT/Apache-2.0 — do not move code between them in either
direction. See [NOTICE.md](NOTICE.md).

## Licence

By contributing you agree that your contributions are licensed under the same
terms as the project: MIT or Apache-2.0, at the user's option.

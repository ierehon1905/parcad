# Contributing to ParCAD

ParCAD is early — 0.0.1, one author, and the DSL still moves. Issues, questions
and patches are all welcome, including "I tried to model X and gave up here",
which is the most useful report this project gets:
[docs/DSL_GAPS.md](docs/DSL_GAPS.md) was written from exactly that.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), then
[docs/GOTCHAS.md](docs/GOTCHAS.md). Most of what looks like a bug here has
already been diagnosed once. [CLAUDE.md](CLAUDE.md) is the same material at
length, addressed to a coding agent.

## Setting up

Beyond [README.md](README.md#build-it)'s prerequisites: Python 3.11+ (the
grader's self-test uses `tomllib`) and `patch(1)` on `PATH`.

```bash
cd app && bun install --frozen-lockfile && cd ..   # first: the Rust build shells out to bun
git config core.hooksPath .githooks                # a clone does not get the gate automatically
export TZ=UTC                                      # commits are recorded in UTC; the hook refuses others
```

Development is on macOS. Linux builds the kernel crates in CI, but nobody has
built the OCCT worker or the desktop window there. Windows is unsupported — the
worker's process handling has no Windows arm.

## The gate

```bash
tools/check.sh          # build, tests, the B-rep worker, the eval corpus (~5 min)
tools/check.sh --fast   # kernel crates, editor tests, grader self-test (~4 s)
```

`pre-commit` runs `--fast`, `pre-push` runs everything. `--fast` builds no worker
and runs no geometry, so it is the edit loop, not the claim. Never reach for
`--release` to speed up an edit loop; that profile is LTO'd on purpose.

## What a change has to satisfy

**Never claim geometry is correct without measuring it.** OCCT returns
valid-looking wrong answers routinely — check volume, bounding box or face count.
`eval/cases/` makes a measurement permanent: add a case with each new operation,
and `cargo run -p parcad-eval -- --update` when a changed number is intended.
Read that diff. A case green on a wrong number is worse than no case, which is
what `known_defect` is for.

**Never claim an agent-facing tool works because its output is correct.** Whether
a model *reads* that output is measured separately, by `field/run-suite.sh`, and
it has been wrong every time it was checked.

**Refuse rather than approximate.** Non-uniform scale, blended intersection and
general offsets all `bail!` with a reason. Don't add a "close enough" path.

**Report measured values, not requested ones**, and **make error messages name
the fix.**

## Traps

- **A new op touches six places**: `graph.rs` (variant *and* `children_of`),
  `sdf.rs`, `measure.rs`, `backend.rs`, `app/src/dsl.ts`, and a case in
  `eval/cases/`. A missing `children_of` arm is silent — the node just never gets
  evaluated. Exhaustive matches find the rest.
- **The selector grammar is parsed twice on purpose**, in `selectors.rs` and
  `app/src/selectors.ts`. Neither is the specification; `eval/selectors.json` is,
  and both languages test against it.
- **Every DSL export becomes a reserved word in a saved part.** Adding one is a
  compatibility change.
- **Units are millimetres**, and primitives are centred on the origin.
- **`cargo build` produces a *dev* app whatever the profile** — Tauri's switch is
  the `custom-protocol` feature, not `--release`.
- **The product is ParCAD; every identifier is `parcad`.** The bundle id and the
  project folder are load-bearing: renaming them orphans saved parts.

## Vendored code

`vendor/` is LGPL-2.1 while our crates are MIT/Apache-2.0. Don't move code
between them in either direction, and record every change to a vendored crate in
that directory's `PARCAD-CHANGES.md`. See [NOTICE.md](NOTICE.md).

## Licence

Contributions are licensed as MIT or Apache-2.0, at the user's option.

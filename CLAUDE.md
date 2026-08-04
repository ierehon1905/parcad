# Working on parcad

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first, then
[docs/GOTCHAS.md](docs/GOTCHAS.md). Most of what looks like a bug here has
already been diagnosed once.

## Build & run

```bash
cargo build --release                  # core, CLI, app host — no C++
tools/build-worker.sh                  # B-rep worker; verifies OCCT is optimised
cd app && bunx tauri dev               # the app. From app/. Not --bun.
bun tools/run.ts examples/bracket.js   # DSL script -> intent graph JSON
tools/bench-kernel.sh examples/*.json  # medians, and prints the -O level
```

`cargo build` at the workspace root does **not** build OpenCASCADE — that's what
`default-members` is for. Keep it that way; a cold OCCT build is ~10 minutes.

## Conventions

**Units are millimetres. Always.** `Doc::units` records it so a file can't be
silently misread. Anything else is rejected at the door.

**Primitives are centred on the origin**, placed with a separate `Translate`.
Don't add a primitive with a corner-based origin.

**Comments explain *why*, and are load-bearing.** The existing code documents the
reasoning behind non-obvious choices (why a subprocess, why a post-condition,
why not Release). Match that density — it's the only place some of this
knowledge exists. Don't strip comments when refactoring.

**Error messages name the fix.** Compare `OcctError::Crashed`'s text, or
`worker_path()`'s "build it with … and point PARCAD_OCCT_WORKER at it". An
agent-facing tool whose errors only say what failed is half-built.

**Report measured values, not requested ones.** `deflection_mm` over
`Request::deflection`; tight bounds from geometry over `framing_bounds` from the
graph. `PartReport` exists so nothing has to ask a follow-up question.

**Refuse rather than approximate.** Non-uniform scale, blended intersection and
general offsets all `bail!` with a reason. That is the correct behaviour — do
not add a "close enough" path.

**Never claim geometry is correct without measuring it.** OCCT returns
valid-looking wrong answers routinely. Volume, bounding box, face count — check
one of them. The `-O0` bug survived because the Rust layer was verified and the
C++ underneath was assumed.

## Where things live

| you want to change | file |
|---|---|
| the graph schema / a new op | `crates/parcad-core/src/graph.rs` |
| how an op becomes an SDF | `crates/parcad-core/src/sdf.rs` |
| how an op becomes a B-rep | `crates/parcad-occt/src/backend.rs` |
| crash handling, timeouts, breadcrumbs | `crates/parcad-occt/src/host.rs` |
| the authoring DSL | `app/src/dsl.ts` (shared with `tools/run.ts`) |
| how it looks | `app/src/viewport.ts`, `app/src/outline.ts` |

Adding an op means touching `graph.rs` (variant + `children_of`), `sdf.rs`,
`backend.rs`, and `dsl.ts`. Missing `children_of` is silent — the node just
never gets evaluated.

## Vendored code

`vendor/opencascade` is a fork of the crates.io crate, kept minimal so it can go
upstream. **Every change goes in `vendor/opencascade/PARCAD-CHANGES.md`.** It is
LGPL-2.1 while our crates are MIT/Apache — don't move code between them.

## Git

Nothing is force-pushed to `main`, ever. See the user's global rules.

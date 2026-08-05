# Working on parcad

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), then
[docs/GOTCHAS.md](docs/GOTCHAS.md). Most of what looks like a bug here has
already been diagnosed once.

## Build & run

```bash
cargo build --locked --release         # core, CLI, app host — no C++
tools/build-worker.sh                  # B-rep worker; verifies OCCT is optimised
cd app && bun install --frozen-lockfile && bun run tauri dev  # from app/
bun tools/run.ts examples/bracket.js > /tmp/bracket.json   # DSL -> intent graph
tools/bench-kernel.sh /tmp/bracket.json                    # medians, and the -O level
cargo run -p parcad-eval               # the geometry + refusal corpus
cd app && bun test src                 # editor-side units: the selector grammar
```

**The running app hosts the UI on <http://127.0.0.1:4242>.** A browser there is
the same application as the desktop window, not a cut-down one — same bundle,
same Rust service, same kernel. Under `tauri dev` use <http://localhost:1420>
instead, which proxies `/api` to that port. Move it with `PARCAD_HTTP_PORT` when
running two instances. Nothing in the frontend may branch on which transport it
got; see docs/ARCHITECTURE.md, "One application, two windows".

Root `cargo build` does **not** build OpenCASCADE — that's what `default-members`
is for. Keep it that way; a cold OCCT build is ~10 minutes. Keep the commands
locked: `rust-toolchain.toml`, `Cargo.lock`, `app/bun.lock` are the reproducible
inputs.

**No graph JSON is checked in, deliberately.** `examples/` and `eval/cases/` hold
DSL scripts; generate a graph when you need one. A checked-in graph doesn't fail
when `dsl.ts` changes under it — it just quietly describes an older part. The old
`examples/bracket.json` went three edge treatments stale unnoticed.

## Conventions

**Units are millimetres. Always.** `Doc::units` records it so a file can't be
silently misread. Anything else is rejected at the door.

**Primitives are centred on the origin**, placed with a separate `Translate`.

**Comments explain *why*, and are load-bearing.** They're the only place some of
this knowledge lives (why a subprocess, why a post-condition, why not Release) —
don't strip them when refactoring. Write a *new* one only for genuinely complex
logic, a setup unique to this codebase, or a rare corner case. Restating the next
line is noise.

**Error messages name the fix.** See `OcctError::Crashed`, or `worker_path()`'s
"build it with … and point PARCAD_OCCT_WORKER at it". An agent-facing tool whose
errors only say what failed is half-built.

**Report measured values, not requested ones.** `deflection_mm` over
`Request::deflection`; tight bounds from geometry over `framing_bounds`.
`PartReport` exists so nothing has to ask a follow-up question.

**Refuse rather than approximate.** Non-uniform scale, blended intersection and
general offsets all `bail!` with a reason. That's correct — don't add a "close
enough" path.

**Never claim geometry is correct without measuring it.** OCCT returns
valid-looking wrong answers routinely; check volume, bounding box, or face count.
The `-O0` bug survived because the Rust layer was verified and the C++ under it
assumed. `eval/cases/` makes a measurement permanent: add a case with each new
operation, `--update` when a change is intended. A case that's green on a wrong
recorded number is worse than no case — that's what `known_defect` is for.

## Where things live

| you want to change | file |
|---|---|
| the graph schema / a new op | `crates/parcad-core/src/graph.rs` |
| how an op becomes an SDF | `crates/parcad-core/src/sdf.rs` |
| how an op becomes a B-rep | `crates/parcad-occt/src/backend.rs` |
| crash handling, timeouts, breadcrumbs | `crates/parcad-occt/src/host.rs` |
| the authoring DSL | `app/src/dsl.ts` (shared with `tools/run.ts`) |
| what the app can do at all | `app/src-tauri/src/service.rs` — never a transport file |
| the IPC and HTTP adapters | `app/src-tauri/src/lib.rs`, `app/src-tauri/src/http.rs` |
| how the frontend calls the backend | `app/src/backend.ts` — the only module that knows there are two |
| the selector grammar | `selectors.rs` **and** `app/src/selectors.ts` — see below |
| what the editor marks as you type | `app/src/selector-lint.ts` |
| what a treatment hover says, and offers to edit | `app/src/treatment-info.ts` (content), `treatment-hover.ts` (the extension) |
| how it looks | `app/src/viewport.ts`, `app/src/outline.ts` |
| what "still correct" means | `eval/cases/*.json`, `eval/scripts/*.js` |
| the example parts | `examples/*.js` — indexed in `examples/README.md`, read directly by the app's picker |
| what the DSL makes hard | `docs/DSL_GAPS.md` |

A new op touches `graph.rs` (variant + `children_of`), `sdf.rs`, `backend.rs`,
`dsl.ts`, plus a case in `eval/cases/`. Missing `children_of` is silent — the
node just never gets evaluated.

**The selector grammar is parsed twice on purpose.** The kernel must own a
parser, because a graph can arrive from anywhere and the editor is never the
gate; the editor needs one too, because underlining a bad term on the keystroke
that types it cannot wait for an IPC round trip. Neither copy is the
specification — `eval/selectors.json` is, and both
`agrees_with_the_shared_selector_corpus` (Rust) and `selectors.test.ts` (bun)
run against it. Change a rule in one language only and a test goes red rather
than the editor quietly accepting what the kernel later refuses. Record the
message *and* the span; the span is what the editor underlines.

**An offered source edit must not change the part.** The treatment tooltip
suggests `{ generatedBy: tag }` only when the kernel reports that tag's live
edge set is *exactly* the current target — `EdgeLineage::equivalent_sources`,
equality and not containment. A tag covering these edges and more would treat
edges the author never selected, which is the "refuse rather than approximate"
rule applied to a refactor. Anything the editor offers to write needs that
standard of evidence, measured, not inferred from the source text.

## Vendored code

`vendor/opencascade` is a fork of the crates.io crate, kept minimal so it can go
upstream. **Every change goes in `vendor/opencascade/PARCAD-CHANGES.md`.** It's
LGPL-2.1 while our crates are MIT/Apache — don't move code between them.

## Git

Nothing is force-pushed to `main`, ever. See the user's global rules.

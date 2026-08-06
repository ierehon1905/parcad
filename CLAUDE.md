# Working on parcad

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), then
[docs/GOTCHAS.md](docs/GOTCHAS.md). Most of what looks like a bug here has
already been diagnosed once.

## Build & run

```bash
tools/check.sh                         # everything below that gates a change, in order
tools/check.sh --fast                  # kernel crates only: <1 s warm, ~4 s after an edit
cargo build --locked --release         # core, CLI, app host — no C++
tools/build-worker.sh                  # B-rep worker; verifies OCCT is optimised
cd app && bun install --frozen-lockfile && bun run tauri dev  # from app/
bun tools/run.ts examples/bracket.js > /tmp/bracket.json   # DSL -> intent graph
tools/bench-kernel.sh /tmp/bracket.json                    # medians, and the -O level
cargo run -p parcad-eval               # the geometry + refusal corpus
tools/field-test.sh eval/field/does-the-port-meet.md 4   # ask a small model, over MCP
cd app && bun test src                 # editor-side units: the selector grammar
```

`tools/check.sh` is wired to git — `pre-commit` runs `--fast`, `pre-push` runs
the corpus too — and to Claude Code, as a `Stop` hook in `.claude/settings.json`
that runs in the background and only interrupts on failure. The git side is a
local config and does not travel with a clone:

```bash
git config core.hooksPath .githooks
```

**`--fast` builds no worker and runs no geometry.** It is the kernel crates in
the `test` profile — incremental, opt-level 2 — and it deliberately skips
`parcad-app` (a 5 s sleep in its sandbox test) and `tools/build-worker.sh`
(relinking 26 MB of OpenCASCADE). Both belong to the full run. Release is
compiled hard and slowly on purpose; never reach for it to make an edit-test
loop faster, and see the profile comments in `Cargo.toml` before changing it.

**The running app also serves MCP at <http://127.0.0.1:4242/mcp>** — the same
`service.rs` the UI uses. Scripts from an agent run in `script.rs`'s QuickJS
sandbox, never in the webview; that is a hard rule, and docs/ROADMAP.md records
why. Parts live in one shared folder (`~/Documents/parcad`, or
`PARCAD_PROJECTS_DIR`) that the app, the user and MCP all read and write —
`examples/` only seeds it on first run.

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

**A project is a `.parcad` folder, and `part.js` inside it is the only
authoritative file.** `parcad.json`, `README.md` and `preview.png` beside it are
derived and disposable — the app rewrites them on save, from *measured* values,
and nothing reads them back as fact. A loose `.js` is still a project and must
stay one. Seeding records what it has placed in `.seeded` rather than checking
whether a path is occupied: a part the user moved into a folder of their own is
not a part that is missing. See docs/ARCHITECTURE.md, "Projects are files, not
fixtures".

**No graph JSON is checked in, deliberately.** `examples/` and `eval/cases/` hold
DSL scripts; generate a graph when you need one. A checked-in graph doesn't fail
when `dsl.ts` changes under it — it just quietly describes an older part. The old
`examples/bracket.json` went three edge treatments stale unnoticed.

## Conventions

**Every DSL export becomes a reserved word in a script.** Parts run as
`new Function(...names, source)`, so adding an export called `hole` breaks every
saved part that wrote `const hole = ...`. Adding one is a compatibility change:
prefer a name a part would not choose for a local, and check `examples/` builds.

**Units are millimetres. Always.** `Doc::units` records it so a file can't be
silently misread. Anything else is rejected at the door.

**Primitives are centred on the origin**, placed with a separate `Translate`.

**Styling is Tailwind, and `style.css` holds only tokens.** Every colour, font
and step of the type scale is a `@theme` entry, which Tailwind emits as both a
utility and a plain custom property — that is what lets the CodeMirror theme and
the viewport read the same value instead of keeping a second palette in
TypeScript. Appearance lives on the element as utilities, including for DOM
built in TypeScript; a shared look is a named constant next to the markup
(`BUTTON`, `TOOLTIP`), not a class in a stylesheet. The only CSS rules left are
for elements CodeMirror renders and names itself, because there is nothing there
to put a class on.

Two traps, both of which have already cost a session:

- **A variant cannot be "the base plus a different colour".** Conflicting
  utilities resolve by their order in the generated stylesheet, not in the class
  attribute, so `BUTTON + "bg-accent-deep"` silently keeps whichever background
  Tailwind emitted last. Base states shape; each variant states its own colours.
- **Preflight zeroes every margin**, which removes the `margin: auto` a browser
  uses to centre a modal `<dialog>`. Add `m-auto` back.

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

**And never claim an agent-facing tool works because its output is correct.**
Whether a model *reads* that output is a separate fact, measured separately, and
it has been wrong every time it was checked: a probe flag read inverted, a field
name read as the wrong noun, a tool never called at all. `tools/field-test.sh`
puts a question to a small model over the running app's MCP and keeps the
transcript. Run it before calling anything in docs/PERCEPTION.md done, and read
the transcript rather than the verdict — the most instructive trial on record
got the right answer by quoting the part's own source comment.

## Where things live

| you want to change | file |
|---|---|
| the graph schema / a new op | `crates/parcad-core/src/graph.rs` |
| how an op becomes an SDF | `crates/parcad-core/src/sdf.rs` |
| how an op becomes a B-rep | `crates/parcad-occt/src/backend.rs` |
| crash handling, timeouts, breadcrumbs | `crates/parcad-occt/src/host.rs` |
| the authoring DSL | `app/src/dsl.ts` (shared with `tools/run.ts`) |
| drill and clearance sizes | `METRIC_FASTENERS` in `app/src/dsl.ts` — never a literal in a part |
| what the app can do at all | `app/src-tauri/src/service.rs` — never a transport file |
| the IPC, HTTP and MCP adapters | `app/src-tauri/src/lib.rs`, `http.rs`, `mcp.rs` |
| the sandbox agent scripts run in | `app/src-tauri/src/script.rs` |
| where parts are stored | `app/src-tauri/src/projects.rs` — a `.parcad` folder per part |
| the parts picker: folders, new part, rename, trash | `app/src/project-browser.ts`, rules in `app/src/projects.ts` |
| how the frontend calls the backend | `app/src/backend.ts` — the only module that knows there are two |
| the selector grammar | `selectors.rs` **and** `app/src/selectors.ts` — see below |
| what the editor marks as you type | `app/src/selector-lint.ts` |
| what a treatment hover says, and offers to edit | `app/src/treatment-info.ts` (content), `treatment-hover.ts` (the extension) |
| how it looks | `app/src/viewport.ts`, `app/src/outline.ts` |
| colours, type, the design tokens | `@theme` in `app/src/style.css` — never a literal in a component |
| what "still correct" means | `eval/cases/*.json`, `eval/scripts/*.js` |
| the seed parts | `examples/*.js` — indexed in `examples/README.md`; copied into the project folder on first run, not read by the picker |
| what the DSL makes hard | `docs/DSL_GAPS.md` |
| which op to add next, and why not the others | `docs/OP_ROADMAP.md` |
| which of all the open fronts to do first | `docs/NEXT.md` |
| measuring a part without looking at it | `crates/parcad-core/src/probe.rs`, `thickness.rs` |
| cutting a part open to see inside it | `view.rs`'s `Section`, then `render.rs` for the agent and `app/src/viewport.ts` for the window |
| what an agent can see, and what to tell it instead | `docs/PERCEPTION.md` |
| whether a model can *read* a tool | `eval/field/*.md`, run by `tools/field-test.sh` |

A new op touches `graph.rs` (variant + `children_of`), `sdf.rs`, `measure.rs`
(its bounds), `backend.rs`, `dsl.ts`, plus a case in `eval/cases/`. Missing
`children_of` is silent — the node just never gets evaluated. Rust will find the
other three for you: every one of those matches is exhaustive.

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

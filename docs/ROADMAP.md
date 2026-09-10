# Known gaps and what's next

## Security

### Agent-authored scripts run in QuickJS, not the webview *(closed)*

The blocker was: **scripts run via `new Function` in the webview have full page
access, including the Tauri `invoke` bridge.** Fine for a script a human typed
into their own editor; not fine the moment an agent authors one, which is the
entire point of the project.

`app/src-tauri/src/script.rs` takes QuickJS in Rust rather than a webview worker,
because it needs no window open and shares no origin with the editor. The realm
is created empty: QuickJS without `quickjs-libc` has no `fetch`, no `require`, no
filesystem and no console, and nothing in that file adds a host function. It is
not an allow-list, which would have to be complete to be worth anything.
Asserted rather than described, in `script::tests`:

| probe | result |
|---|---|
| `require`, `fetch`, `process`, `window`, `__TAURI_INTERNALS__`, `Deno`, `Bun`, … | `ReferenceError` — the name is simply not there |
| `while (true) {}` | stopped at 5 s by an interrupt handler |
| unbounded allocation | `out of memory` inside the script, at a 64 MB cap |

Two things still have the original shape. `tools/run.ts` runs a script under bun
with the user's own shell privileges — acceptable for a developer running a file
they are looking at, and not a path an agent reaches. And the *editor* still uses
`new Function`, correct for a human typing into it but not for a script pasted
from a model; routing it through the same Rust sandbox would cost an IPC round
trip per debounced build.

## Kernel capabilities not yet reachable

Each is blocked on a specific missing binding, not on design, and all three
`bail!` with an explanation rather than approximating.

| want | blocked on |
|---|---|
| general outward `offset` on a boolean result | `BRepOffsetAPI_MakeOffsetShape` — absent from `opencascade-sys` too, so it needs a new cxx binding plus C++ shim |
| non-uniform `scale` | `gp_GTrsf` / `BRepBuilderAPI_GTransform`, unbound |
| blended **intersection** | the bindings' intersection reports no new edges to fillet |

## Product

- **Provenance selectors and persistent feature history.** `generatedBy` follows
  created Boolean section edges plus OCCT's modified and deleted edge relations
  across union and difference, and combines with facts such as `curve: "circle"`.
  Extend that history through fillet, offset, shell, transforms and intersection
  before treating it as universal. An ordinal selector (`nth`) may follow as an
  explicitly fragile, sorted tie-breaker; `op#3.edge[2]` will not be an authored
  reference.
- **Complete edge-treatment recipes.** Vertex provenance/adjacency, G2 blends,
  setback/miter/blend corners, chamfer two-distance and distance/angle modes, and
  variable/chord/asymmetric fillets — only with exact kernel support, and
  declared-but-unsupported recipes must keep failing explicitly.
- **Bidirectional feature inspection.** Extend the treatment-target preview to
  replacement faces and history through changed curves, rather than guessing
  after an operation replaces topology.
- **Binary geometry channel.** Tauri `invoke` serialises the mesh to JSON; for
  the bracket that is ~12 000 triangles plus 67 edge polylines, and it shows up
  as the gap between kernel time and observed time. Unblocked now the debounce no
  longer hides it.
- ~~**Sidecar packaging.**~~ *Done.* A `.app` copied out of the build tree and run
  with `PARCAD_OCCT_WORKER` unset measures the bracket at 55074.791 mm³,
  watertight, the corpus value. Not done: signing, notarisation, an updater, or
  any target that is not this machine's — see [NEXT.md](NEXT.md) §1.
- ~~**Debounce is the bottleneck.**~~ *Done.* It was 350 ms, chosen when a rebuild
  cost 300–450 ms; kernel time is now 76 ms (enclosure) to ~130 ms (bracket), so
  the timer outlasted the geometry. `DEBOUNCE_MS` in `app/src/engine.ts` is 120.

## Perception (the actual thesis)

The reason for keeping the implicit backend is that it can answer questions a
B-rep cannot. Barely started:

- An **eval harness**. The deterministic half exists: `crates/parcad-eval` runs
  `eval/cases/*.json` through both backends and checks measurements, topology
  counts and required refusals. Missing is the half that makes it a *perception*
  harness — ablation of the channels an agent is given, so "does it still get
  this right without renders?" is a measurable question. That needs a bundle
  format, a question set with expected answers, and a runner that grades a
  model's replies.
- **Non-human perception modes**: field probes, slice stacks, ray arrays,
  printability fields. Cheap on an SDF and impossible on a B-rep, and the point
  of having both.

### Built: one live session the agent can drive

MCP used to be stateless by construction — `save_project` wrote a file and
stopped, no event reached any window, and the open editor was a correct but stale
view of the folder until a reload. `app/src-tauri/src/session.rs` holds the
session beside `service.rs`: `name`, `script`, `revision`, the id of whichever
viewer originated the change, and a broadcast channel. `open_project`,
`set_script` and `get_session` adapt it on the MCP side; SSE at
`/api/session/events` serves browsers and a Tauri event serves the webview.

Viewers push their own document back on the existing evaluation debounce, or
`get_session` lies. Each viewer ignores broadcasts it originated, which keeps two
tabs and the desktop window in sync without an echo loop — and an *identical*
push is a no-op on the host, which stops a viewer re-pushing an applied remote
change from rippling forever.

The conflict rule is deliberately not a lock: **an agent edit is an ordinary
edit**, dispatched into CodeMirror like typing, so it lands in the normal undo
history and Cmd-Z takes it back. A lock would have to be explained; undo does
not. Whether a model *drives* the session is measured separately, by
`eval/field/change-the-open-part.md`, one trial at a time — there is one screen
and parallel trials fight over it.

Ordered, because each is the prerequisite for the next. **1** *(done)*:
`service::evaluate` produces one `EvaluationSnapshot` and all three transports
serialise it, so the editor's own `Report` — a second definition on the far side
of a transport, the divergence `service.rs` was extracted to stop — is gone, and
`mcp.rs` defines no type that describes geometry. **2** *(done)*: renders,
sections and region maps as fields on that snapshot rather than further bespoke
tools; a section view is a render-time half-plane and needs no new op. Then:

3. **Compare.** The loop an agent runs is inspect → plan → modify → evaluate →
   *compare* → verify → explain, and there is no diff. Two graphs in,
   geometry/topology/measurement delta out.
4. **Lineage through fillet, offset, shell and transforms** — the provenance
   bullet above. `equivalent_tags` is only as sound as the history under it.
5. **Refusals that name the *right* fix.** `docs/GOTCHAS.md` lists places where
   the message we emit is confidently wrong: a blended union of face-touching
   solids aborts the kernel and blames the radius, which is not the problem. An
   agent reads that, believes it, and retries. Each gotcha with a wrong message
   is a bug in the harness, not a note for a human.

### A render must depict the backend that was measured *(closed)*

The same bug as `1861964 Fix mesh preview geometry`, found again the moment
renders became reachable by an agent rather than by the CLI. Renders came off the
distance field whatever had been measured, and a blended union is a polynomial
smooth-minimum there and a rolling-ball fillet in the exact kernel. Measured on
`examples/bracket.js`:

| | the part | the picture of it |
|---|---|---|
| size | 80.00 × 60.00 × 44.00 mm | 81.50 × 63.00 × 44.17 mm |
| volume | 55 074.79 mm³ | 56 218.85 mm³ |

3 mm of material in Y that is not there, and a bounding box grown *outward* —
precisely what `growth_slip()` refuses when a real fillet does it. Worse, the
field refuses `Fillet` and `Chamfer` outright, so most of `examples/` could not
be drawn at all. `render::raster` rasterises the evaluated mesh into the same
`GeometryBuffer` the raymarcher produces, so shading, occlusion, outlines,
`model_point` and tag attribution are code that already existed and cannot drift
from it.

`a_rastered_view_lands_where_the_raymarched_one_does` compares silhouette
coverage between the two renderers across all seven views; it caught a half-pixel
sampling offset in the first draft — 4% of coverage, and a picture that looks
entirely correct while being systematically shifted. Region *attribution* still
asks the distance field whose surface a point is on, so a fillet — having no
field — owns nothing and its material falls to `unclaimed`, reported in
`unattributed_treatments` rather than handed to a neighbouring tag: a
confidently wrong legend is worse than a short one.

### Reach — which clients can talk to this at all

The surface being correct and the surface being *reachable* are separate facts.
Asked on 2026-08-07 as a choice between two proposals — embed a terminal in the
app, or polish the web UI so an agent's built-in browser can drive it — and both
were declined, for reasons that outlive the question.

**A terminal in the app is rejected.** It buys no capability: an agent already
reaches every `service.rs` function over MCP, and since `session.rs` an agent
edit lands in the open editor as an ordinary edit that Cmd-Z reverses. Against
that, a pty is a full shell inside the process — the exact access `script.rs`
creates an empty QuickJS realm to deny. And it answers neither of CLAUDE.md's two
questions.

**Polishing the UI as an agent channel is rejected for a sharper reason: it is a
pixel channel for a reader that is bad at pixels** — the whole of
[PERCEPTION.md](PERCEPTION.md), and its CADSmith result: a frame that passed
every vision check by an Opus judge while containing gaps three fixed views
cannot resolve. A model reading the window gets strictly *less* than one calling
`evaluate_part`, which returns the snapshot as text, as structured content and as
PNGs in one reply. The premise is sound as a *fact* — Claude Code's desktop
browser (July 2026) opens localhost origins directly, so `http://127.0.0.1:4242`
needs no work from us — and the correct reading is that **the browser is where
the human looks.** The agent's job ends at the file;
`eval/field/put-it-where-i-can-open-it.md` grades that chain.

**What is actually blocked is a transport, and it is small.** The ChatGPT desktop
app, Codex CLI and IDE extension share one MCP config, but the desktop app
[cannot reliably reach a local Streamable HTTP MCP server on
macOS](https://github.com/openai/codex/issues/13920) — handshake and decode
failures, which no front-end work touches. A stdio-to-HTTP shim, or documenting
`npx mcp-remote` beside the `claude mcp add` line in [GOTCHAS.md](GOTCHAS.md),
converts one whole client from unreachable to working. Hours rather than weeks.

If there is a browser deliverable worth building later, it is a URL that opens
one named part for the *user* — and it must be a bare origin plus session state,
because that desktop browser refuses a path or query on localhost.

## Reference numbers

These live in `eval/cases/*.json` and are checked rather than described:

```bash
cargo run -p parcad-eval              # every case, both backends
cargo run -p parcad-eval -- --update  # re-record after an intended change
```

The table that used to sit here was hand-maintained and had drifted — it claimed
12 018 triangles and 31 faces for the bracket where the part now measures 6 838
and 28, and a 26 mm enclosure that is 28 mm. That is the whole argument for the
corpus: nobody edits a prose table when a face count moves.

Where the two backends disagree, the implicit column's error is not a bug — it is
dual contouring at the chosen depth, and each case carries a looser tolerance for
that path than for the exact one. It is also exactly why the B-rep backend
exists. The corpus asserts the refusals too: non-uniform scale, blended
intersection, inward offset and a wrong `expect({ count })` must each fail with a
named variant *and* with the words a reader needs to fix it, because "refuse
rather than approximate" is worth nothing if the refusal does not say what to do
instead.

### Edge treatments now verify their own work *(closed)*

Found by the corpus on its first run. `box(10,10,10).edges(">Z").fillet(r)`:

| r | before | after |
|---|---|---|
| 2, 4 | correct; 10 × 10 × 10, volume falls | unchanged |
| 5 | SIGSEGV — caught, typed, breadcrumbed | unchanged; working as designed |
| 8 | **14.95 × 14.10 × 10.54** — a fillet that grew the solid | refused, naming the radius |

`offset` had `offset_slip()`; fillet and chamfer had nothing. They now share
`growth_slip()`, the same idea one-sided: a fillet removes material at a convex
edge and fills a concave one, and a chamfer only cuts, so neither can move a
bounding-box extreme outward. Containment is a fact about the result rather than
a guess about the input, which is what makes it preferable to an allow-list.
Guarded by `eval/cases/fillet-must-not-grow.json` and a unit test in
`backend.rs`.

Worth noting what the right answer was *not*: a 10 mm cube. A rolling ball of
radius 8 does not fit those edges at all, so refusing is the whole of the correct
behaviour — the first draft of that eval case asserted a cube and was wrong for a
second reason.

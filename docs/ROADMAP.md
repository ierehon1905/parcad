# Known gaps and what's next

## Security

### Agent-authored scripts run in QuickJS, not the webview *(closed)*

The blocker was: **scripts run via `new Function` in the webview have full page
access, including the Tauri `invoke` bridge.** Fine for a script a human typed
into their own editor; not fine the moment an agent authors one, which is the
entire point of the project.

`app/src-tauri/src/script.rs` takes the second of the two options that were on
the table — QuickJS in Rust — rather than a webview worker, because it needs no
window open and shares no origin with the editor. The realm is created empty:
QuickJS without `quickjs-libc` has no `fetch`, no `require`, no filesystem and
no console, and nothing in that file adds a host function. It is not an
allow-list, which would have to be complete to be worth anything.

Asserted rather than described, in `script::tests`:

| probe | result |
|---|---|
| `require`, `fetch`, `process`, `window`, `__TAURI_INTERNALS__`, `Deno`, `Bun`, … | `ReferenceError` — the name is simply not there |
| `while (true) {}` | stopped at 5 s by an interrupt handler |
| unbounded allocation | `out of memory` inside the script, at a 64 MB cap |

**`tools/run.ts` still has the original shape of the problem** — it runs a script
under bun with the user's own shell privileges. That is acceptable for a
developer running a file they are looking at, and it is not a path an agent
reaches; the MCP server does not use it.

The remaining gap is the *editor*: the webview still uses `new Function`, which
is correct for a human typing into it, but a script pasted from a model into
that editor is not sandboxed. Routing the editor through the same Rust sandbox
would close it, at the cost of an IPC round trip per keystroke-debounced build.

## Kernel capabilities not yet reachable

Each of these is blocked on a specific missing binding, not on design.

| want | blocked on |
|---|---|
| general outward `offset` on a boolean result | `BRepOffsetAPI_MakeOffsetShape` — absent from `opencascade-sys` too, so it needs a new cxx binding plus C++ shim |
| non-uniform `scale` | `gp_GTrsf` / `BRepBuilderAPI_GTransform`, unbound |
| blended **intersection** | the bindings' intersection reports no new edges to fillet |

All three currently `bail!` with an explanation rather than approximating.

## Product

- **Provenance selectors and persistent feature history.** Per-edge fillets use
  spatial (`>Z and >Y and |X`), topological, and Boolean provenance selectors,
  not B-rep indices; `expect({ count })` also makes a changed match count fail
  loudly. `generatedBy: "mount_holes"` follows created Boolean section edges
  plus OCCT modified and deleted edge relations across union and difference,
  then combines with facts such as `curve: "circle"` and `role: "hole"`.
  Extend that history through fillet, offset, shell, transforms and intersection
  before treating it as universal.
  An ordinal result selector (`nth`) may follow as an explicitly fragile,
  sorted tie-breaker; `op#3.edge[2]` will not be an authored reference.
- **Complete edge-treatment recipes.** Equal-distance chamfers and G1
  rolling-ball fillets share edge-set and extrema-selected corner-vertex
  targets. Add vertex provenance/adjacency, G2 smooth blends,
  setback/miter/blend corners, chamfer two-distance and distance/angle modes,
  and variable/chord/asymmetric fillets only with exact kernel support;
  declared-but-unsupported recipes must keep failing explicitly.
- **Bidirectional feature inspection.** Selecting a treatment method previews
  its exact input edges in the viewport; final curves returned by the exact
  fillet/chamfer history now focus that source call on click. Extend this to
  replacement faces and history through changed curves, rather than guessing
  after an operation replaces topology.
- **Sidecar packaging.** `parcad-occt-worker` is copied beside the dev binaries
  by `tools/build-worker.sh`, but is *not* declared as a Tauri sidecar. A
  bundled `.app` will not find it. (`bundle.active` is currently `false`.)
- **Debounce is now the bottleneck.** The editor waits 350 ms after the last
  keystroke, chosen when a rebuild cost 300–450 ms. Kernel time is now 76 ms
  (enclosure) to ~130 ms (bracket), so you wait longer for the timer than for
  the geometry. ~120 ms would roughly halve felt latency.
- **Binary geometry channel.** Tauri `invoke` serialises the mesh to JSON;
  for the bracket that's ~12 000 triangles plus 67 edge polylines, and it shows
  up as the gap between kernel time and observed time. Only worth doing after
  the debounce change.

## Perception (the actual thesis)

The reason for keeping the implicit backend is that it can answer questions a
B-rep cannot. Barely started:

- An **eval harness**. The deterministic half exists: `crates/parcad-eval`
  runs `eval/cases/*.json` through both backends and checks measurements,
  topology counts and required refusals. Still missing is the half that makes
  it a *perception* harness — ablation of the channels an agent is given, so
  "does it still get this right without renders?" is a measurable question.
  That needs a bundle format (which artifacts a case exposes), a question set
  with expected answers, and a runner that grades a model's replies.
- **Non-human perception modes**: field probes, slice stacks, ray arrays,
  printability fields. These are cheap on an SDF and impossible on a B-rep, and
  they are the point of having both.

### The agent surface exists, and it cannot see

`app/src-tauri/src/mcp.rs` landed as soon as the sandbox closed, so a model now
reaches the same `service` functions the two windows do. Every one of its tools
returns numbers. Meanwhile `parcad-core` already contains an ambient-occlusion
raymarcher (`render.rs`), seven consistently-framed standard views (`view.rs`),
and a false-coloured tag-region map whose legend reports `visible: false` for a
tag that is genuinely in the model but hidden from this angle (`tags.rs`) — and
the only caller of any of it is `parcad-cli`. `service.rs` has no render entry
point, so neither window nor any agent can ask for one.

It also cannot *act on* what the user is looking at. MCP is stateless by
construction: every tool call carries its whole script, evaluates it in
isolation, and returns. `save_project` writes a file and stops — no event
reaches the webview or the browser, so a part an agent saves appears in the
picker only on reload. While an agent works, the open window is a correct but
stale view of the folder.

**Decided, not built: one live session the agent can drive.** The app process
already owns both ends — the MCP handler and the window are the same process,
and browsers are on the same axum host — so the plumbing is short:

- Session state in Rust (`name`, `script`, `revision`, and the id of whichever
  viewer originated the change), plus a broadcast channel.
- `open_project`, `set_script` and `get_session` on the MCP side, so an agent
  can change what is on screen *and* read what the user has since typed.
- Viewers push their own document back on the existing evaluation debounce, or
  `get_session` lies. Each viewer ignores broadcasts it originated, which is
  what keeps two browser tabs and the desktop window in sync without an echo
  loop.
- SSE at `/api/session/events` for browsers; a Tauri event for the webview.
  One broadcast, two transports, same rule as everything else here.

The conflict rule is deliberately not a lock: **an agent edit is an ordinary
edit.** It lands in CodeMirror's normal undo history, so Cmd-Z takes it back and
a user who disagrees with a change reverses it the way they reverse their own.
A lock would have to be explained; undo does not.

Ordered, because each item is the prerequisite for the next:

1. **One evaluation artifact** *(done)*. `service::evaluate` now produces one
   `EvaluationSnapshot` and all three transports serialise it: MCP as the reply
   to `evaluate_part`, IPC and HTTP as the `snapshot` beside the mesh. The
   editor's own `Report` — a second definition living on the far side of a
   transport, which is the same divergence `service.rs` was extracted to stop —
   is gone with it, and `unused_nodes` moved into the snapshot so the window
   still has the one number it derived that nothing else reported. `mcp.rs` now
   defines no type that describes geometry.
2. **Renders, sections and region maps as fields on that snapshot**, not as
   further bespoke tools. A section view is a render-time half-plane, needs no
   new op, and `docs/DSL_GAPS.md` records it as the thing most missed while
   writing all twelve examples. Renders and region maps are done; see below for
   what drawing them off the wrong backend cost.
3. **Compare.** The loop in `AI_CAD_PLATFORM.md` is inspect → plan → modify →
   evaluate → *compare* → verify → explain, and there is no diff. Two graphs in,
   geometry/topology/measurement delta out.
4. **Lineage through fillet, offset, shell and transforms** — see the provenance
   bullet above. `equivalent_tags` is only ever as sound as the history under
   it.
### A render must depict the backend that was measured *(closed)*

The same bug as `1861964 Fix mesh preview geometry`, found again the moment
renders became reachable by an agent rather than by the CLI. Renders came off
the distance field whatever had been measured, and a blended union is a
polynomial smooth-minimum there and a rolling-ball fillet in the exact kernel.
Measured on `examples/bracket.js`:

| | the part | the picture of it |
|---|---|---|
| size | 80.00 × 60.00 × 44.00 mm | 81.50 × 63.00 × 44.17 mm |
| volume | 55 074.79 mm³ | 56 218.85 mm³ |

3 mm of material in Y that is not there, and a bounding box grown *outward* —
which is precisely what `growth_slip()` refuses when a real fillet does it.
Worse, the field refuses `Fillet` and `Chamfer` outright, so most of
`examples/` could not be drawn at all.

`render::raster` fixes it by rasterising the evaluated mesh into the same
`GeometryBuffer` the raymarcher produces, so shading, ambient occlusion,
silhouette outlines, `model_point` and tag attribution are all the code that
already existed and cannot drift from it. The picture is now of the same
evaluation as the numbers beside it.

Two things worth keeping:

- `a_rastered_view_lands_where_the_raymarched_one_does` compares silhouette
  coverage between the two renderers across all seven views. It caught a
  half-pixel sampling offset in the first draft — 4% of coverage, and a picture
  that looks entirely correct while being systematically shifted. `view`'s
  promise that a feature lands on comparable pixels now spans two renderers.
- Region *attribution* still asks the distance field whose surface a point is
  on, so a fillet — having no field — owns nothing and its material falls to
  `unclaimed`. Reported in `unattributed_treatments` rather than handed to a
  neighbouring tag, because a confidently wrong legend is worse than a short
  one.

5. **Refusals that name the *right* fix.** `docs/GOTCHAS.md` is a list of places
   where the message we already emit is confidently wrong: a blended union of
   face-touching solids aborts the kernel and blames the radius, which is not
   the problem. An agent reads that, believes it, and retries. Each gotcha with
   a wrong message is a bug in the harness, not a note for a human.

## Reference numbers

These now live in `eval/cases/*.json` and are checked rather than described:

```bash
cargo run -p parcad-eval              # every case, both backends
cargo run -p parcad-eval -- --update  # re-record after an intended change
```

The table that used to sit here was hand-maintained and had drifted — it
claimed 12 018 triangles and 31 faces for the bracket where the part now
measures 6 838 and 28, and a 26 mm enclosure that is 28 mm. That is the whole
argument for the corpus: nobody edits a prose table when a face count moves.

Where the two backends disagree, the implicit column's error is not a bug — it
is dual contouring at the chosen depth, and each case carries a looser
tolerance for that path than for the exact one. It is also exactly why the
B-rep backend exists.

The corpus also asserts the refusals. Non-uniform scale, blended intersection,
inward offset and a wrong `expect({ count })` must each fail with a named
variant *and* with the words a reader needs to fix it, because "refuse rather
than approximate" is worth nothing if the refusal does not say what to do
instead.

### Edge treatments now verify their own work *(closed)*

Found by the corpus on its first run. `box(10,10,10).edges(">Z").fillet(r)`:

| r | before | after |
|---|---|---|
| 2, 4 | correct; 10 × 10 × 10, volume falls | unchanged |
| 5 | SIGSEGV — caught, typed, breadcrumbed | unchanged; working as designed |
| 8 | **14.95 × 14.10 × 10.54** — a fillet that grew the solid | refused, naming the radius |

`offset` had `offset_slip()`; fillet and chamfer had nothing. They now share
`growth_slip()`, which is the same idea one-sided: a fillet removes material at
a convex edge and fills a concave one, and a chamfer only cuts, so neither can
move a bounding-box extreme outward. Containment is a fact about the result
rather than a guess about the input, which is what makes it preferable to an
allow-list. Guarded by `eval/cases/fillet-must-not-grow.json` and a unit test
in `backend.rs`.

Worth noting what the right answer was *not*: a 10 mm cube. A rolling ball of
radius 8 does not fit those edges at all, so refusing is the whole of the
correct behaviour — the first draft of that eval case asserted a cube and was
wrong for a second reason.

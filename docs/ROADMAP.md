# Known gaps and what's next

## Security — do this before MCP lands

**Scripts run via `new Function` in the webview have full page access, including
the Tauri `invoke` bridge.** That is fine for scripts a human typed. It is not
fine the moment an agent authors them, which is the entire point of the project.

Fix before shipping an MCP server: run scripts in a worker with no Tauri API
exposed, or move execution to QuickJS in Rust. `tools/run.ts` has the same shape
of problem but runs under bun where the blast radius is the user's own shell.

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

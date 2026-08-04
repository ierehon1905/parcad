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
  rolling-ball fillets share the selected-edge target contract. Add G2 smooth
  blends, setback/miter/blend corners, chamfer two-distance and distance/angle
  modes, and variable/chord/asymmetric fillets only with exact kernel support;
  declared-but-unsupported recipes must keep failing explicitly.
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

- An **eval harness** — a bucket of parts with expected measurements, plus
  ablation of perception channels, so "does the agent still get this right
  without renders?" is a measurable question.
- **Non-human perception modes**: field probes, slice stacks, ray arrays,
  printability fields. These are cheap on an SDF and impossible on a B-rep, and
  they are the point of having both.

## Reference numbers

Verified geometry (implicit vs B-rep):

| case | implicit | B-rep |
|---|---|---|
| bracket | 81.51 × 63.01 × 44.17, 50 006 tris | 80.00 × 60.00 × 44.00, 12 018 tris, 31 faces / 154 edges |
| enclosure | 70.00 × 45.00 × 26.01, 71 792 tris | 70.00 × 45.00 × 26.00, 3 976 tris, 52 faces / 220 edges |
| `box.rotate("z",45)` | 21.17, vol 1199.82 | 21.21, vol 1200.00 (exact) |
| `box.scale(2)` | vol 7999.22 | vol 8000.00 (exact) |
| `box(64,39,22).shell(2)` | vol 17107.27 | vol 17112.00 (exact) |

The implicit column's error is not a bug — it is dual contouring at the chosen
depth. It is also exactly why the B-rep backend exists.

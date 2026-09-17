# Architecture

Written for maintainers and coding agents working on this codebase, not as an
introduction to CAD.

## The intent graph is the only contract

`crates/parcad-core/src/graph.rs` defines a `Doc`: an arena of `Node`s, a
`root`, and units (always `"mm"`). Nothing in it knows how a shape is computed.
That is the whole design.

```
  script (TS)  ──build()──>  Doc (JSON)  ──occt lower──>  TopoDS_Shape  ──┬──> mesh ──> renders, regions, STL
                                                                        ├──> edges, STEP
                                                                        └──> probes, thickness, tag extents
```

Consequences worth internalising:

- **Primitives are centred on the origin.** Placement is a separate `Translate`
  node. This makes symmetry the default.
- **Tags and selectors, never indices.** A `tag` on a node is the anchor for
  named regions. For exact edge operations, a directional selector such as
  `>Z and >Y and |X` is resolved against the current B-rep; it is not an OCCT
  edge number. A tag resolves to a *set of faces* — the faces of the tagged
  node's result, followed through every later boolean, blend, fillet, chamfer
  and rigid motion by the kernel's own history, and lost through offset, shell
  and intersection, which report none. This avoids pretending that a transient
  topology index can survive a model edit.
- **A node's meaning can differ per backend and that is allowed** — but it must
  be documented. See "blend" below.

### A graph says what it needs

The graph and the host that builds it are often different builds: the editor
bundle of a source tree posting to an installed app, `tools/run.ts` output
handed to a released CLI, a worker from `PARCAD_OCCT_WORKER`. Parcad 0.0.6 read
a part with a rounded section corner as `the graph is not valid: invalid type:
map, expected an array of length 2`, which took half an hour to trace to the
version. So:

- **`requires` lists the features a graph uses that an older host cannot
  read**, each as `{ feature, after, what }`, stamped by `build()` in
  `app/src/dsl.ts` from `GRAPH_FEATURES`. Only features a graph actually uses
  are listed, and only features newer than the version that started reading
  `requires` exist at all.
- **A host refuses an id it does not know, in the writer's words**:
  `crates/parcad-core/src/envelope.rs`, `parse_doc`, used by every transport,
  the CLI and the corpus, and by the worker for its own copy. "This part uses
  sections with rounded corners, arcs or splines, which needs a parcad released
  after 0.0.6. This host is parcad …: update it, or point the client at a newer
  host."
- **Everything else that fails to read names the node, the field and the
  fix** — an unknown op, a missing field, a value of the wrong shape — and a
  field no slot reads is refused rather than dropped: serde ignored
  `"chamfer": 1` on a cylinder and built a plain one, which is how a newer
  host's optional field would have arrived at an older one.

**Features, not a schema number.** A version integer would say *that* a host
is too old, never *what* it lacks; it would lock every new graph out of an old
host, including the many that use nothing new; and it would have to be bumped
by hand, where the source tree already carries a version (0.0.6) that does not
describe what it can read. A feature id is exact, and its entry carries the
words and the release a host that has never heard of it needs to print. The
reverse direction is free while features are only added: a graph with no
`requires` is an older writer's and reads as it always did. A feature whose
meaning ever changes gets a new id, and the host can refuse the old one with
its own words.

**What already-shipped hosts do:** 0.0.6 and earlier ignore `requires`, so they
still fail with serde's text on the first entry they cannot parse, and still
silently drop a field they have no slot for. Nothing can change that
retroactively; every host from this one on fails by name. `GRAPH_FEATURES`'s
`after` is the last release that cannot read a feature — a fact at the time it
is written, not a guess at the next version number.

Adding a graph feature is a row in `GRAPH_FEATURES` and an id in
`envelope::FEATURES`; `every_graph_feature_is_stamped_and_read` in
`parcad-host` fails until both exist and a script that uses the feature stamps
it. `eval/cases/graph-states-what-it-needs.json` pins what one part stamps.

### A part may be several bodies, and they stay several

A script that returns an object of shapes — `return { base, lid }` — builds a
graph whose root is `Op::Bodies`, a list of `{ name, child }` over each body's
own subgraph. That op is the root or nothing: `Doc::topo_order` refuses one
anywhere else, because a boolean or a treatment over the group would have to
fuse the bodies to mean anything, and "these stay separate" is the one thing
the op says. Nothing joins, mates or constrains one body to another; each sits
where its script placed it, which is the cheap end of multi-body and all of it
that exists. Joints, assembly hierarchy and instancing across parts are out by
decision, in [OP_ROADMAP.md](OP_ROADMAP.md).

The B-rep worker builds each body with the same `build_node` a one-solid part
uses, meshes and checks each one on its own — a body whose mesh does not close
is refused naming the body — and concatenates the meshes into one reply, with
each body's run of triangles recorded in `Success::bodies`. Host-side,
`parcad_occt::measure_bodies` cuts that run back out and measures it with the
code that measures a whole part, so a body's volume and the part's are the same
kind of number and the bodies sum to the part; the same function serves the
app's snapshot (`named_bodies`) and the eval corpus. Every pair is measured on
the exact solids with `fit_between`, the measurement behind `check_fit`, and
reported as `between_bodies`: `clear` by a clearance, `touching`, or
`interfering` by a shared volume — a clip drawn through the body it clips onto
is a design error the number states outright. STEP writes the compound, which
OCCT's writer turns into one solid per body; 3MF writes each body's welded
triangles as its own named object, so a slicer can place the halves apart
(`parcad_occt::body_meshes`, `parcad_core::threemf`); STL writes every body's
triangles into one file, and `export_part`'s `body` picks one body out by
rebuilding the graph with that body as the root.

The part-level `bodies` count keeps its meaning — free-standing closed pieces
of the whole mesh, one for a part — and for a part in named bodies it should
equal their number. Which of those pieces is an *accident* is a per-body
question: each `named_bodies` entry carries `pieces`, one when that body is
intact, which is what separates a union that quietly left a body in two from a
second body that was meant. Tags, `on:`/`between:` queries and directional
extrema all resolve inside the body being built and never across two, by
construction: a body never enters another body's lineage, and a treatment
cannot take the group as its child.

Region maps, `tag_extents`, `probe_part` and `measure_wall_thickness` run
body by body: a crossing, a point and a thin spot each name the `body` they
are in.

### `blend` is a boolean, then a fillet

A `blend` is "do the boolean, then fillet the edges the boolean created": no
bulge, a true rolling-ball fillet, and the seam's faces carry the lineage of
the faces the edge lay between. The other defensible reading of "round this
join by 6 mm" — a smooth-minimum of two distance fields, which bulges by
roughly `k/4` — was what the deleted implicit backend built, and the reason
renders once depicted a part 3 mm wider than the one measured. There is one
reading now, and everything a user or an agent sees or measures is it.

### Selected edge and corner treatments are exact-only

`shape.edges(">Z and >Y and |X").fillet(2)` is a B-rep operation: `>Z` and `<Z`
select edges whose centres are at a global directional extreme, `|X` restricts
the result to straight edges parallel to X, terms combine with `and`, and a
selector that finds nothing is rejected rather than silently falling back to an
array position. Selection is on the kernel's logical edges, never on mesh
vertices that happen to be nearby.

`shape.vertices(">X and >Y and >Z").fillet(2)` selects the single corner at those
three extrema. The 3D OCCT builder accepts edges, not a vertex, so the backend
expands that vertex to its exact incident edge set just before construction — the
source intent stays a corner while the builder does its real rolling-ball corner
construction. Vertex selectors support only extrema at present: their Boolean
provenance has not yet been made durable.

Object selectors express topology facts that directional extrema cannot. The
bracket's
`shape.edges({ curve: "circle", role: "hole", adjacentTo: { faceNormal: "+z" } })`
selects every closed circular inner loop bordering an upward-facing face, keeping
the four upper hole rims coupled to their geometry as holes move or multiply
while excluding lower rims, outside bosses and open blend arcs.

Every selectable edge also knows the angle its two faces make, read from the
face's outward normal and the edge's direction of travel in that face's wire:
`dihedral: "convex"` is an outside corner — "break every edge" —
`"concave"` an inside one, `"smooth"` no corner at all, the boundary an
earlier fillet left or a cylinder's seam. `parallel: "z"` is the object form
of `|Z`, and `longerThan` keeps a sliver out of a cosmetic pass. **A fillet or
chamfer leaves smooth edges out unless asked for them by name**: a rolling
ball has nothing to build on where two faces already meet tangent, and
selecting one was the commonest way a cosmetic pass failed, with a message
that named only a count. Every treatment failure and every `expect` mismatch
now lists the edges it means, shortest first — position, length, curve kind,
corner — which is what turns a bisection into an edit.

A query can also say *where* to look. `on: "lip"` keeps only edges bounding a
face of that feature, and with `on` the `at` extrema are measured among the
feature's own edges rather than the document's, so `{ on: "cup", at: { z:
"max" } }` is the cup's top rim wherever the rest of the part reaches.
`between: ["arm", "hub"]` keeps edges with one face from each: the seam, and
only the seam, after the union that made it. Both read the face lineage
described under tags above; a name with no live faces is refused with the
names that have some. A face merged from two features at a coplanar join
carries both names, so `on` reaches across such a join; the boundary the
merge erased is not one a name can keep.

### Why a plain edge index is the wrong foundation

`op#3.edge[2]` is an ordering convention, not an identity: a Boolean, a fillet or
a parameter edit can split, merge, delete or reorder the output edges under it.
Not a quirk of this kernel — Fusion's `BRepEdge.tempId` is documented as valid
only while the owning body is unmodified, and Onshape answers it the way this
project does, with provenance (`qCreatedBy(featureId, EntityType.EDGE)`). `>X`
has the same problem in a smaller way: it means "whatever is rightmost in the
current result", an identity only when rightmost is genuinely the design
intent.

- [CadQuery selectors](https://cadquery.readthedocs.io/en/stable/selectors.html)
- [Fusion temporary edge IDs](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/BRepEdge_tempId.htm)
- [Onshape FeatureScript query examples](https://cad.onshape.com/FsDoc/library.html)
- [OpenCascade topology naming and evolution](https://dev.opencascade.org/doc/refman/html/_t_naming_8hxx.html)

### Selector strength is intentional

No one selector kind is universally strongest: each encodes a different kind of
design intent. An authored reference should say why an entity matters, not where
it happened to land in a kernel-owned list.

| kind | example | use it when | stability |
|---|---|---|---|
| spatial | `>X and |Z` | the intent is truly "the rightmost vertical edge" | relative to the whole current part |
| topology | `{ curve: "circle", role: "hole", adjacentTo: { faceNormal: "+z" } }` | the intent is a class of feature such as upper hole rims | relative to the current topology |
| provenance | `{ generatedBy: "mount_holes", curve: "circle", role: "hole" }` | the intent is specifically an entity produced by a named Boolean operation | carried through later exact Booleans; refused after unsupported history |
| corner vertex | `>X and >Y and >Z` | the intent is every incident edge at one outer corner | relative to the current part; no Boolean vertex lineage yet |
| ordinal *(planned, fragile)* | `{ generatedBy: "mount_holes", orderBy: "radius", nth: 1 }` | a deliberately ordered tie-break is really part of the intent | changes if the result set changes |
| viewport ID | `edge@42` or `vertex@13` | inspect, hover, debug, or copy a selector suggestion | one evaluation only; never script input |
| target-preview ID | `target@3.0` or `target-vertex@3.0` | show the exact pre-treatment target under an editor cursor | one preview request only; never script input |
| treatment history | `edge@42 → .fillet(…)` | click a final curve generated by an exact fillet/chamfer to focus its source call | one evaluation only; never script input |

The source-facing reference is therefore a **named operation tag**, combined with
a topology query and an optional cardinality assertion:

```js
drilled.edges({
  generatedBy: "mount_holes",
  curve: "circle",
  role: "hole",
  adjacentTo: { faceNormal: "+z" },
}).expect({ count: 4 }).fillet(0.8);
```

`expect({ count })` validates the selector result in the exact backend before the
operation changes the solid. `generatedBy` follows created Boolean section edges
plus OCCT's modified and deleted edge relations through union and difference;
other topology-changing operations clear that relation and an attempted lookup
fails rather than guessing. A raw ordinal remains an escape hatch only: it must
be explicitly sorted, visually marked fragile, and never replace a semantic or
provenance reference.

### Source-to-viewport target preview

Placing the editor caret anywhere in a treatment's selector, expectation, or
method call (`.fillet`, `.chamfer`, `.smooth`, `.squircle`) asks the isolated
worker to resolve that node's target against its **input** B-rep, and draws those
exact curves in gold over the finished part; a `vertices(...)` target also draws
the selected corner point. Against the input, because a treatment usually
consumes or replaces its input edges and highlighting final `edge@…` values would
suggest a false correspondence. The preview follows parent translations,
rotations and scales, repeats when a graph node is reused, and is strictly
diagnostic: source selectors remain the authored reference, `target@…` and
`target-vertex@…` are ephemeral inspection IDs.

The editor takes a treatment's source range from its parsed syntax — it does not
persist the location in the intent graph or infer it from `Error.stack`, whose
format varies between browser JavaScript and Tauri's WebKit runtime.

### Edge treatments are a feature family, not an edge-selector trick

The current API applies a treatment to an **edge-set or corner-vertex target**:

```js
shape.edges({ generatedBy: "mount_holes", role: "hole" })
  .fillet(0.8, { continuity: "tangent", corner: "rollingBall" });
shape.edges(">Z and >Y and |X").chamfer(1.0);
shape.vertices(">X and >Y and >Z").fillet(1.0);
```

Selection (`edges(...)` or `vertices(...)`) is intentionally separate from the
geometric recipe: selectors say *what* changes; the treatment says *how*.

| source operation | exact support | extensible recipe |
|---|---|---|
| `.fillet(radius)` | tangent (G1), rolling-ball | G2 curvature, setback, variable/chord/asymmetric size laws |
| `.chamfer(distance)` | equal-distance, planar corner | two-distance, distance/angle, miter and blend corners |
| `.smooth(radius)` / `.squircle(radius)` | declared intent; rejected until a true G2 surface builder exists | curvature-continuous blend shape and weights |

`continuity: "tangent"` is G1; `"curvature"` is G2. The fillet `corner` describes
how several selected edges are solved at a shared vertex: `"rollingBall"` or
`"setback"`. Chamfers independently have `"chamfer"`, `"miter"` and `"blend"`
corner policies. Unsupported but valid recipes are rejected by the exact backend
rather than being accepted and ignored.

`squircle` is an ergonomic alias for `.smooth()`, not a claim that this 3D edge
treatment is a 2D superellipse. In CAD terms the intended property is G2
curvature continuity; using that name keeps scripts and agent explanations
geometrically honest.

Keeping the target separate leaves further targets as first-class additions
shared by every treatment rather than special forms of `EdgeSelector`: edge sets
and corner vertices today, and a planned full round, which replaces a centre face
with a transition between two side-face sets.

## One kernel, and what the second one left behind

For most of this project's life there were two backends: an implicit one —
every op lowered to a signed distance function, evaluated and meshed by
[fidget](https://github.com/mkeeter/fidget) — and the exact one. The implicit
path was *total*, it always returned something, and that was its undoing: it
refused every edge treatment, loft, sweep and helix by name (33 of 41 parts
in a real project folder), read `blend` as a different shape, dropped fillets
from every probe and wall-thickness answer and called the result an upper
bound. Once probes, rays, thickness, tag extents and renders all ran on the
exact solid and the field suite read them SOUND, `sdf.rs`, fidget and the
`implicit` half of every eval case were deleted (`2c08506d`).

What stayed in `parcad-core` from that side is the part that never depended on
a field: the rasteriser and its section capping (`render.rs`), the region
colouring (`tags.rs`), mesh statistics and STL (`mesh.rs`), and the ambient
occlusion pass (`occlusion.rs`, ported from fidget and MPL-2.0 for that reason
— NOTICE.md). Everything geometric lives in `parcad-occt`, behind the worker.

`parcad_occt::evaluate` is allowed to refuse, and the CLI's `--brep` and
`--depth` still parse and say they do nothing.

## The B-rep kernel runs in a child process

`crates/parcad-occt/src/host.rs` is the reason this crate is split in half.

OCCT signals failure by throwing `Standard_Failure`, which the fillet boundary
catches in C++ — the vendored wrapper's `ParcadEdgeTreatment::build` — returning
the kernel's own words and turning every known abort into a refusal. What no
catch helps with: OCCT can also segfault on degenerate input and spin for minutes
on a pathological fillet, and `catch_unwind` covers neither. So the worker is a
separate binary, and every outcome — including the ones OCCT expresses by killing
the process — comes back as an `OcctError` variant:

- `Rejected` — the kernel understood and said no. Actionable.
- `Crashed` — it died. `stage` is the last **breadcrumb** the worker printed to
  stderr before going down, which is the only evidence of what killed it.
- `TimedOut` — still running past the deadline (default 20 s).
- `Host` — we couldn't start it or couldn't read its reply.

For an agent that will routinely ask for a fillet larger than the material can
take, this is the difference between a bad answer and a dead session.

**The reply travels via a temp file, not stdout.** OCCT writes progress banners
to stdout — the STEP writer alone emits hundreds of kilobytes — so stdout is a
channel we neither control nor can parse. The worker announces each reply with
an `@reply <path>` line on stderr, beside its breadcrumbs.

### One worker serves many requests

A worker used to be started per request and exit with its reply. It now
serves: started once, kept in a pool of at most two while idle (a window and
an agent are two callers at once), and handed request after request as
`Frame`s, one per line of stdin. A worker that crashes or is killed at the
deadline leaves the pool; the next request starts a fresh one, and the
supervision is what it was — `host.rs`'s tests still drive a shim that hangs
and one that dies, plus one that answers twice and one that dies between
answers.

What the worker keeps between requests is a **build cache**
(`backend::BuildCache`): every subtree it built, keyed by what the subtree is
— its ops and tags with child indices blanked, and its children's keys — and
by the translation pushed down into it. An edit rebuilds the nodes it changed
and the operations above them; a probe or a thickness sweep of the part just
built builds nothing; an evaluate of an unchanged part is its mesh alone. A hit
counts as a use of everything beneath it, and what two builds have not touched
is dropped. Measured on `examples/plate-stand.js`, whose blended union is
2.7 s of its 3.2 s build: the same part again 475 ms, a ray probe 122 ms,
and a change to the slot chamfer at the root 697 ms against 3.2 s cold. An
edit to a peg still costs the blend above it, which is where the time is. A
cached build answers an edited graph exactly as a fresh worker does — status,
volume, topology and every tag extent, measured on five example parts each
edited at the root, edited at a leaf, and unchanged.

The process floor — spawn, request, reply file — was never the cost: 11 ms
warm on the smallest part. The tag extents and face report added to every
reply are, at 14 ms on the bracket and 123 ms on the plate stand, and both
are computed once per body rather than once per tag.

### The feature split

`parcad-occt` builds by default as *just the host half* — no C++ at all. The
`kernel` feature pulls in `opencascade` and enables `backend.rs` and the worker
binary. The app and CLI depend on it with default features, so an everyday
`cargo build` never touches OpenCASCADE. `tools/build-worker.sh` builds the
worker and drops it beside every application binary, which is where
`host::worker_path()` looks (override with `PARCAD_OCCT_WORKER`).

## Post-condition verification instead of an allow-list

OCCT's `offset_surface` will happily return a **valid-looking but wrong** solid:
it silently drops bodies on booleans, and `offset_surface(+3)` can hand back an
inside-out shape whose next offset then runs backwards. None of this raises.

`backend.rs` therefore checks its own work. Offsetting by `d` must move every
bounding-box extreme by exactly `d` — `offset_slip()` measures the violation and
the operation refuses above `SLIP_TOLERANCE_MM` (0.05), naming the millimetre
error. This turns a class of silent wrongness into a loud refusal, which is
strictly better than an allow-list of "shapes we think are safe".

Fillet and chamfer get the same treatment through `growth_slip()`, one-sided. A
fillet removes material at a convex edge and fills a concave one; a chamfer only
cuts. Neither can move a bounding-box extreme outward, whatever the shape or the
selection, so the result must fit inside the solid it started from. Without this,
`box(10,10,10).edges(">Z").fillet(8)` returned a 14.95 × 14.10 × 10.54 mm shape
and no error — a radius that does not fit produces a wrong answer rather than a
refusal. At radius 5 the same call reports not-done, which the caught boundary
turns into a refusal naming the largest radius measured to build; both outcomes
are in the eval corpus, because they are different failures.

Two lowerings exist for the same reason:

- **`Offset` of a cuboid** is lowered as *grow + fillet all 12 edges to r*. That
  is not an approximation of a Minkowski sum — for a box it **is** the Minkowski
  sum, exactly, and it avoids `offset_surface` entirely.
- **`Shell`** is `solid − solid.offset_surface(−t)`. The wrapper's `hollow()`
  needs a face to open and produced a *shrunken solid* rather than a hollow one
  (measured: 64×39×22 became a solid 60×35×18 with six faces).

## A walled loft, and lofts through fitted sections

A loft whose sections are each one closed `{ fit }` over the same number of
points is not handed to `ThruSections`. `skinned.rs` builds it as Piegl &
Tiller's *compatible skinning* (§10.3), with the arithmetic in
`parcad-core/src/skin.rs`:

- **One parameter per authored point, shared by every section** — each
  section's chord-length parameters, averaged — so point `i` is at the same
  `u` in every section and the author's pairing is the surface's.
- **One knot vector for every section.** Each is fitted by least squares on a
  *periodic* cubic (`PeriodicFit`): a closed outline has no seam to
  constrain, and with parameters and knots shared, one factorisation fits all
  of them. The knots follow the parameters (Piegl & Tiller eq. 9.69, closed),
  so every span holds the same number of points: uniform knots over points
  whose spacing varied 4:1 left spans empty long before the points ran out,
  and a pleated section that fits to 0.0007 mm was refused at 0.163. The span
  count is the fewest that holds every section's tolerance, measured point
  to curve, without a loop: doubling from 4, then halving the gap to the last
  count that fell short, up to the most the points allow. Nothing is unified
  or inserted afterwards. The periodic curve is handed to OCCT as the clamped
  cubic equal to it.
- **The surface interpolates each column of poles across the sections**, at
  `v` = the section's height scaled to [0, 1]. Interpolation reproduces a
  linear function, so height is exactly linear in `v`: every horizontal plane
  cuts the skin along one `v` iso-curve, and a floor or a rim is one.
- **The faces are made and sewn here** (`opencascade::skin::Skinner`): a band
  per stretch between sections, each on its own segment of the surface —
  split even when smooth, because the mesher took 102 s on one lamp skin as a
  single face and 18.7 s in bands, and segmented because a boolean's
  `UnifySameDomain` welds faces that share a surface back into one (docs/GOTCHAS.md)
  — and flat ends bounded by the skin's own iso-curves.

`loft(sections, { wall })` adds the inside, and the pairing is what it is for.
Two skins lofted independently through a section and its inset each bulge
their own way between sections: a 15-section lamp built as two smooth lofts
and a cut measured **0.001 mm** of wall in eight places. Here the inside is
fitted on the same parameters as the outside, on a refinement of its knots
(twice as many spans, and four or eight times when the wall measured below
misses — an offset turns tighter than its curve, at a convex tip by the whole
wall), through targets the backend computes from the *built outside*: at
several parameters between each pair of points and at rows between each pair
of sections, the point `t` along the outside's inward surface normal. Each
target is the offset of the outside at `(u, v')`, with `v'` solved so the
target lies at its row's height, so both skins keep height linear in `v` and
a floor or a rim is still one iso-curve of each; `u` is shared exactly, and
`v'` differs from the row's `v` by at most `t` of height. There is no limit
on lean: a bowl's floor 9° off flat walls like a vase's side. What the
geometry forbids is refused by name: where the profile bends tighter than
`t`, the inside's height stops rising with the outside's and would fold. Below
a closed end no rows are made — the inside starts at its floor — and at an
open end the outside is continued by its end span to be stepped from.
(A horizontal step of `t` is only `t · cos φ` thick square to a wall leaning
`φ`: the fitted lamp built that way measured 1.211 mm for 1.6.) The step is
then corrected twice by the wall it made, measured square to the outside at
each target's foot, and the result answers to one measurement:
`Skinner::measure_wall_reaching`, from a grid of points of the inside to the
nearest point of the outside, searched only within a knot span (or the `t`
of height the step can shift) of the *same* parameters — the matching
stretch of wall, never a ray that crosses the cavity. At an open end the foot
is held to the edge and the distance is taken along its normal, which is the
wall continued.

A ruled loft reports `facet_sag_mm`, how flat its facets are: the furthest
its walls lie from the smooth loft through the same sections, measured both
ways — on a skinned loft between the ruled and the smooth surface through
the same pole rows (`skin::facet_sag`, a Gauss-Newton foot from the same
parameters), otherwise between the `ThruSections` solid and the smooth one
OCCT builds through the same wires, from a grid on every face to the other's
boundary. Two sections have none: the smooth loft through them is the ruled
one. It is the number for what a person judged by eye: the owner saw faint
horizontal lines on a 768 px render of the 41-section ruled lamp, whose
shade measures 0.139 mm (its cavity 0.176 mm, which the part reports as the
larger), and none on the smooth lamp. `ruled-facet-sag` holds it to a closed
form.

A closed `{ fit }` section anywhere else — an extrusion, a revolve — is
fitted the same way, as a loft of one section (`skinned::fit_closed`), and
built as a B-spline edge with its deviation measured again on the edge.

Whether the skins cross themselves or each other is decided on their poles
before they are sewn (`skin_crossing`, docs/VALIDITY_CHECKS.md): height being
linear in `v` makes it a question about plane curves at each height, which
halving Bézier patches settles exactly in milliseconds, where the kernel's
self-intersection check spent 16 s on one pleated shade.

The measured range is reported as `loft_wall_mm`. Thinner than 95 % of `t`
anywhere is refused, naming where; thicker than `t` by more than 5 % or the
sections' own fit tolerance, whichever is more, is refused too, because a
smooth inside rounds a turn it cannot follow and the author allowed curves
that much slack. Ends are open by default — the wall ends in a flat ring, a
lampshade or a sleeve — and `bottom: "closed"` / `top: "closed"` put a floor
`t` thick there, cut from the inside at `v = t / height`. A wall takes only
fitted sections: corners and arcs have no points to step, and their inset is
already `inset()`. An open end on a wall that nearly lies flat is a knife
edge, since the ring is level: `walled-shallow-shade` (11° off flat) measures
a 0.098 mm feather at its rim.

Which way the solid faces is stated, not classified: the skinner is told the
outward direction at one point of the outside (`set_outward`), turns the
sewn solid to match and checks the face it presents there.
`BRepLib::OrientClosedSolid` shoots one ray from a face and trusts its
farthest crossing; through a pleated shell of dozens of walls 1.2 mm apart it
missed one and reversed a solid that was right, and every report read the
volume unsigned. A skinned loft is therefore not classified afterwards; every
other construction is (`facing_outward`), and the mesh backstop in `serve.rs`
refuses any closed mesh shell wound against its nesting, whatever built it
(docs/VALIDITY_CHECKS.md).

## Surfaces

A shape may be a *surface*: faces with no inside, and free edges — edges
bordered by one face — where it ends. The owner reversed the old "surface
modelling is out of scope" because some forms are only natural as surfaces: a
shade of separate blades has free edges, and a closed skin never can. What a
shape is, is measured rather than declared (`surfaces::kind_of`, from
`Shape::census`): no solid is a surface, a solid with no loose face is a solid,
and anything else is refused at the body as *mixed* — return the two as
separate bodies. `crates/parcad-occt/src/surfaces.rs` builds every surface op;
the OCCT half is `vendor/opencascade/include/surfacing.hxx`.

**Making one.** `surface_extrude`, `surface_revolve`, `surface_loft` and
`surface_sweep` take a *curve*: the same entries a section takes
(`section::resolve_curve`), open by default — from the first corner to the last,
nothing joining them — or closed. A surface's outward normal lies to the right
of its curve's direction of travel (seen from +Z for a loft or an extrusion,
in the profile plane for a sweep or a revolve), which is the outside of an
anticlockwise outline; every builder probes the built shell near the curve's
start and turns it over if it faces the other way, and refuses a shell the
kernel's checker rejects. `thicken`'s `out` and `in` are read against that
normal. A loft of curves that are each one `{ fit }` over the same number of
points is skinned like a fitted solid loft — one shared knot vector, one
parameter per point (`skin::PeriodicFit` closed, `open_fit::OpenFit` open) — and
cut into faces of at most 32 knot spans along u as well as at every curve,
each face its own segment of the surface: the mesher took 470 s on a pleated
shade in whole bands and 5 s in segments, and `UnifySameDomain` samples a
face's whole underlying surface for every neighbour it compares.

A smooth surface loft reaches past its curves where the shape it describes
turns or swells between them — its extreme lies between two curves, not on
one — so `graph::surface_loft_extent` widens the curves' box by the furthest
one curve's box moves from the next's, and the backend measures the built
surface against that box and refuses past it, naming the side.

**Editing one.** `patch` fills every closed loop the selected free edges make:
the exact plane where the loop is flat, otherwise `BRepOffsetAPI_MakeFilling`,
whose boundary's distance from the edges is measured and refused past the
mesher's 0.01 mm (`patch_gap_mm`). `stitch` sews at a tolerance the script
states and closes the result into a solid only when it has no free edge and is
one sheet — measured, with `solid: true` to refuse otherwise, listing the free
edges. `trim` splits with `BRepAlgoAPI_Splitter` and keeps the pieces on one
side of the tool: a plane's normal side, a solid's inside, a surface's front,
each piece classified at a point inside it; a tool that cuts nothing, keeps
everything or keeps nothing is refused. `split` keeps every piece.
`offset_surface` and `thicken` run `BRepOffset_MakeOffset` (skin mode, and
thickening), shell by shell; `thicken` "both" offsets the surface back by half
first. Before either builds, the surface is sampled on a grid in every face
and refused where the offset would fold: where the distance times the
curvature toward it reaches one, naming the radius. After, the wall is read
at every sample as the largest ball centred midway through it
(`NearestBoundary`): a wall built right reads its thickness; a fold of the
surface running into the wall reads thinner; a skin the kernel dropped reads
nothing. More than 1 % off anywhere is refused, and the range is
`thickened_mm`. Names follow each face through all of these by the kernel's
own history (`FaceHistory`, with the side walls a thickened edge makes
inheriting its face's names); `generatedBy` does not.

**What a surface reports.** No volume, watertightness, bed or print fit:
`kind: "surface"` and `surface` — area, free edges and their length, loops,
sheets. The worker's backstop for a surface is the one that fits it: the
mesh's own open edges must run as long as the free edges do, chords of them,
and no longer. Booleans, fillets, chamfers, offset and shell refuse a surface
operand naming `.thicken(t)`; `measure_wall_thickness` refuses a surface and
skips one in a mixed part; STL and 3MF refuse, STEP writes it. `probe_part`
never calls a point on a surface `material` and lists every place a ray
passes through one. Between a surface and another body `between_bodies`
reports `clear`, `touching` or `crossing`, the last when splitting the surface
by the other body cuts it.

## Meshing: weld before you measure

OCCT triangulates **face by face**, so every shared edge arrives as two
coincident vertex copies, and a perfectly closed solid then reports "NOT
watertight — 2238 bad edges". `Tessellation::weld(1e-3)` merges coincident
vertices and drops degenerate triangles; both the CLI and the app call it before
computing mass properties or stats.

## Edges are the kernel's, not inferred

The viewport draws OCCT's own edge curves rather than guessing creases in screen
space. `serve.rs::edge_curves()` keys each edge by its rounded polyline and
keeps those bordering **two or more distinct faces**, and a surface's free
edges, which one face visits once — that filter is what removes *seam edges*,
where a closed surface's parameterisation wraps and one face visits the edge
twice. Seams are topologically real but visually an artifact: without the
filter every bore has a line down it. For the bracket this takes 77 curves down
to 67. An edge between two faces that meet with the same tangent plane and the
same curvature along it (`Shape::split_edges`) is dropped for the same reason:
it is a split inside one surface — a loft's bands, a torus in two halves, the
flank strips of a thread — and a fillet's boundary, where the curvature jumps,
stays.

## The app

### One application, two windows, and one with none

The capabilities and both hosts are `crates/parcad-host`, a crate with no
Tauri in it. `app/src-tauri` embeds it and adds a window and an IPC adapter;
`parcad serve` embeds it and adds nothing, which is what the Homebrew formula
runs as a service. The frontend reaches the router through an `Assets`
provider — Tauri's resolver in the app, a copy of `app/dist` compiled into the
CLI — so there is still exactly one frontend build and no host can serve a
different one. `parcad tools` and `parcad call` are then an MCP client of
whichever host is running: the CLI does not carry a second list of tools, it
asks `/mcp` for the one there is.


The desktop process hosts its own UI and API on `127.0.0.1:4242`
(`PARCAD_HTTP_PORT` to move it). A browser pointed at that port is not a reduced
build: it loads the same frontend bundle Tauri embeds and calls the same Rust
functions the webview calls, so there is no such thing as a browser-only
limitation to learn.

```
  webview  ──Tauri IPC──┐
                        ├──> crates/parcad-host/src/service.rs ──> core / OCCT worker
  browser  ──HTTP────────┘
```

`service.rs` owns every capability; `lib.rs` and `http.rs` are adapters that add
nothing. That split is load-bearing: the browser build was previously a frozen
`dev-geometry.json` fixture, and each capability the editor gated on `inTauri` —
editing, backend choice, exact target preview, both exports — was a difference
the user had to discover. The frontend now reaches the backend only through
`app/src/backend.ts`, which chooses a transport and nothing else.

There is also exactly one description of an evaluation. `service::evaluate`
builds an `EvaluationSnapshot` — size, bounds, mass, topology counts, mesh
quality, tags, treatments, unused nodes, the backend that measured it — and every
transport serialises that same value: MCP as the reply to `evaluate_part`, the
two windows as the `snapshot` field beside the mesh they draw. No transport, and
nothing above `backend.ts`, computes a measurement of its own. Both halves of
that rule have been broken — `mcp.rs` rebuilding the summary from raw graph JSON
and looking for `smooth` and `squircle` nodes, which have never been ops; the
editor assembling its own from a raw `PartReport`.

Three constraints on the HTTP half, each deliberate:

- **Loopback only.** The endpoint evaluates arbitrary intent graphs, which means
  spawning the OCCT worker. It is a local tool, never bound to a routable
  address.
- **No CORS headers.** Their absence *is* the access control: another origin can
  send a JSON POST but its preflight fails, so it can never read a reply.
- **No caller-supplied paths.** IPC writes an export where the desktop user
  pointed; HTTP returns bytes and the browser saves them. A path parameter on a
  socket is an arbitrary-write primitive.

The one honest difference left is where an export lands — a file on the desktop,
a download in the browser. Same bytes, same kernel; a property of the host, not
of the model.

### A third transport with no host: the playground

`vite build --mode playground` builds the same frontend as a static site, and
`backend.ts` then answers every call inside the tab instead of over a socket:

```
  webview  ──Tauri IPC──┐
  browser  ──HTTP───────┼──> service.rs ──> parcad_evaluation ──┐
  agent    ──MCP────────┘                  OCCT worker process ├─ parcad_occt::serve
  playground ──Web Worker──> parcad-wasm.wasm ─────────────────┘
```

The kernel is not a reimplementation. `crates/parcad-wasm` compiles
`parcad_occt::serve::run` — the function the native worker's stdin loop calls —
and `parcad_evaluation::evaluated`, the function `service::evaluate` calls, with
the patched OpenCASCADE, to one WebAssembly module (`playground/build-kernel.sh`).
Five exported calls mirror the five HTTP routes the editor uses: evaluate,
inspect an edge target, export STL, 3MF and STEP. The same Node build of the
worker is what the eval corpus measures, and it passes it (playground/README.md
has every number that moved).

What `host.rs` does for a process, `app/src/page/kernel.ts` does for a Web
Worker: one call at a time, the last `@stage` breadcrumb kept, a worker that
traps terminated and reported as a crash naming that stage, one still running
at the deadline (60 s, the desktop's 20 s times the measured slowdown)
terminated as timed out, and the next call starting a fresh worker from the
module already compiled. The module is downloaded after first paint, and the
viewport says what is downloading and how big it is.

The project folder is IndexedDB (`app/src/page/store.ts`), seeded from
`examples/` at build time under the names `projects.rs` gives them and answering
in the same shapes. What needs a host is absent rather than dimmed: no MCP
endpoint answers, so the titlebar chip never appears; there is no shared session
to subscribe to; an export is a download, which the browser transport already
did. Nothing outside `backend.ts` knows which of the three it got — the loading
state is a signal that simply never fires under a host.

Under `tauri dev` the UI comes from Vite on 1420, which proxies `/api` to the
app's port. That keeps every frontend call same-origin, which is what lets the
app ship no CORS configuration at all.

### A third caller: MCP

`/mcp` on the same port is the same application again, for a model rather than a
person. It reaches `service.rs` through the same functions, so a tool cannot do
something the UI cannot, or measure it differently.

```
  webview  ──Tauri IPC──┐
  browser  ──HTTP───────┼──> service.rs ──> core / OCCT worker
  agent    ──MCP────────┘
```

Two things are shaped by the caller being a model, both from CLAUDE.md and both
worth more here than anywhere else — a model cannot ask a follow-up question and
cannot look at the screen:

- **Measured values, never requested ones.** `evaluate_part` returns the
  deflection the mesher achieved, bounds taken from the geometry, and real face
  and edge counts, so nothing has to be inferred from the input.
- **Refusals name the fix.** The service layer's messages are passed through
  whole. A fillet that does not fit answers with the millimetres it overshot by
  and what to change, rather than "operation failed".

The window says whether that caller is there, because an agent reaches the same
`service.rs` and writes to the same project folder: the part being edited can be
replaced by a caller the user cannot see. `mcp.rs` records what it observes — a
request arrived, a client handshook and named itself, a session was closed — and
`service::mcp_status()` hands the same answer to both windows. Deliberately an
observation and not a claim: a client killed at the terminal never says goodbye,
so the status carries the age of the last request and drops a session quiet for
fifteen minutes rather than asserting a connection nobody has heard from. The
count is of *sessions* for the same reason — one client that reconnects opens a
second, and the endpoint cannot tell that from a second client.

Selector work is where an agent needs the most help, so it gets two tools with no
UI equivalent: `check_selector` parses a term and returns the error *and* its
span without touching geometry, and `inspect_treatment_target` resolves a
fillet's input edges against the shape *before* that fillet runs — the same
question the editor's gold target preview answers, asked in text.

### The part inside a chat

A client that speaks MCP Apps shows `evaluate_part`'s part in 3D beside the
call. The tool's `_meta.ui.resourceUri` names `ui://parcad/viewer`, which
`read_resource` answers with `viewer.html` from the same frontend build the
host serves (`vite.viewer.config.ts` inlines everything into that one file,
because the client's iframe may load nothing). The page is
`app/src/viewer/main.ts`: the client hands it the call's `script`, the page
calls `view_part` for the mesh, and draws it with the window's own `Viewport`.
`view_part` is marked `visibility: ["app"]`, so a client keeps it from the
model; it runs the same cached build `evaluate_part` just made.

The mesh goes through the client, not a socket, so its size matters.
`view_part` sends the mesh as Draco and the rest of the window's reply (edges,
faces, the snapshot) as zstd JSON, both base64: the twisted planter's 5.3 MB of
JSON arrives as 0.53 MB, the wash bottle's 4.7 MB as 0.43, the bracket's 0.31
as 0.034. Zstd alone reached 2.0, 2.7 and 0.08. Draco holds each vertex to 14
bits of the part's extent, 0.005 mm on the planter, inside the mesher's own
0.01; it reorders triangles, so each vertex carries its kernel face number and
the page rebuilds the runs the viewport colours and picks by. `draco-core`
(pure Rust) encodes the planter in about 40 ms. A part over 300 000 triangles
is refused.

The page decodes Draco with three's plain-JavaScript decoder: an MCP Apps
host's default policy allows inline script and not WebAssembly. That decoder
and the page's own bundle travel inside `viewer.html` as zstd, unpacked by
`fzstd` and inserted as scripts, so the resource is 0.44 MB instead of 1.5.

To try it without a chat client, run the reference host from
`modelcontextprotocol/ext-apps` (`examples/basic-host`, `SERVERS=...`) against
a `parcad serve` on a spare port. It connects from the browser, and `/mcp`
sends no CORS headers, so put a proxy that adds them in between.

### Scripts from a model run in QuickJS

The editor builds a graph with `new Function` in the webview, which is fine for a
script a human typed. It is not fine for one a model wrote: that code would run
in the page, with the Tauri bridge and the user's session in reach.

`script.rs` therefore evaluates agent-authored scripts in an embedded QuickJS
realm with no host functions at all. There is no `fetch`, `require`, filesystem,
or console to remove — `quickjs-libc` is not linked and nothing adds them back.
What QuickJS *can* still do is never return or allocate without bound, so a
counted work budget and a 64 MB cap turn both into ordinary refusals. The
budget counts interpreter steps, not seconds, so a part builds or is refused
identically on any machine under any load; a script raises it for itself with
`scriptBudget(n)`, so every route that builds the part agrees. GOTCHAS.md has
the measurements. See ROADMAP.md for what this closed and what it did not.

The DSL those scripts run against is `app/src/dsl.ts`, bundled into the binary by
`build.rs` at compile time. Not a committed copy: a generated artifact that is
checked in does not fail when its source changes under it, and a stale one here
would tell an agent that an operation exists which the kernel no longer has.

### Projects are files, not fixtures

`projects.rs` owns one directory — `~/Documents/parcad`, or `PARCAD_PROJECTS_DIR`
— shared by all three callers. The parts that ship are *seeded* into it on first
run and are then ordinary projects: editable, renamable, deletable. Seeding only
ever adds what is missing, so a part the user deletes stays deleted.

One project is a `.parcad` folder:

```text
~/Documents/parcad/
├─ Mounts/                  an ordinary folder; projects nest
│  ├─ Bracket.parcad/       one project
│  │  ├─ part.js            the source, and the only authoritative file in it
│  │  ├─ parcad.json        title and tags
│  │  ├─ README.md          what the part is, from measured values
│  │  └─ preview.png        the viewport at the last save
│  └─ motor-mount.js        a loose script is a project too
└─ .trash/                  where a removed project goes
```

The extension is on the *folder* so a bare `ls` says what each entry is without
descending into it — the layout exists to be read by an agent that landed in the
directory. Nothing is registered as a macOS package: hiding the innards from
Finder would also hide them from the readers this format is for.

**`part.js` is the source of truth; everything beside it is derived.** Deleting
the README or the preview loses nothing — the next save rewrites them from the
report the app just measured, naming which kernel measured it. Nothing is cached:
a stored report is a stale measurement that looks fresh, which is precisely the
confident wrong answer the rest of this codebase refuses.

A loose `foo.js` stays a project, so an agent or a person can drop a file in
without ceremony; the picker offers to convert one, which is the only thing that
ever changes a project's form. Removal is a move into `.trash`, not an unlink:
these are the user's own files, and the thing standing between one and a mis-click
is a dialog they have already learned to dismiss.

This is why the picker reads the folder over the API instead of globbing
`examples/` at build time: an "example" a user cannot open, change and save back
is a different kind of object from the part they are about to make, and the
difference is invisible until they try. It also gives an agent somewhere to put
its work — `save_project` writes to the folder the picker lists, so a part
written over MCP is one reload away from being on screen.

A project name is a path *inside that folder* and nothing else. `safe()` splits on
`/` and refuses a segment that is empty, starts with a dot, carries a `.js` or
`.parcad` extension, or is not exactly one path component — because two of the
three callers are a socket and a model. It refuses rather than sanitising:
correcting a name toward a valid one is a guess about what the caller meant, and
that guess is what a traversal bug is made of. `app/src/projects.ts` repeats the
check so the picker can mark a bad name on the keystroke that types it, the same
arrangement as the selector grammar — `safe()` is the copy that has to be right.

- `app/src/dsl.ts` is the authoring layer and lives in TypeScript, not Rust.
  That's what lets `tools/run.ts` (bun) and the webview run *the same* DSL and
  hand the same JSON to the same core.
- `app/src/viewport.ts` draws the solid with `MeshStandardMaterial`, the
  kernel's own edge lines and a silhouette outline pass. A second look — smooth
  Lambert shading with a triangle overlay and no edges — is still in the file
  for geometry that arrives without edge curves, which nothing now sends; the
  kernel toggle that used to ask for it is gone.
- Timings the app reports include serialising the geometry across whichever
  transport asked — the Tauri IPC bridge or the HTTP host — which for the bracket
  (~12 000 triangles + 67 edge curves) is a real fraction of the total.

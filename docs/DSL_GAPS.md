# DSL gaps

What writing `examples/` ran into, modelling twelve standard mechanical parts
(flange, pillow block, motor mount, manifold, tee, pulley, extrusion, V-block,
coupler, standoff, heat sink, knob).

**§0 is what the language cannot express at all** — parts that were wanted and
are absent because no operation produces the shape. **§1 onward is friction**:
things that model correctly but cost more than they should. All twelve parts
build exactly and are measured in `eval/cases/`, so nothing from §1 on is a
blocker. Entries since fixed are kept and marked, with what the fix was: a closed
gap is the most useful record when judging whether the next one is real.

---

## 0. Not expressible: the feature is missing

**Corrected once already.** This section said a cone was out of reach because
`opencascade-sys` binds no `BRepPrimAPI_MakeCone`. Wrong layer: a cone is a
revolved triangle, and `BRepPrimAPI_MakeRevol` *is* bound — as are `MakePrism`
and `ThruSections`, with `Edge`, `Wire` and `Face` to build a section from. The
kernel was never the constraint; our graph was. Before recording something as
impossible, check the layer that would implement it, not the layer above.

### Since fixed, by the same route

- **a revolved section** — `Op::Revolve`; `cone()` and `countersink()` came with
  it.
- **extrude an authored outline** — `Op::Extrude`, as `extrude(profile, h)` and
  `ngon(sides, size, h, { across })`. Convex outlines only, the same rule as a
  revolve section; `hex-prism` checks it against the closed form for a hexagon.
- **mirror** — `Op::Mirror`, as `.mirror("x")`. A composition rather than a
  binding: `gp_Trsf::SetMirror` is bound only for an *axis*, a half turn about a
  line rather than a reflection in a plane, and the reflection is that half turn
  composed with a point inversion — both bound, both exact. `mirrored-hand` is a
  chiral part whose bounding box tells the two apart, because their volumes
  cannot.
- **section / cut-away for inspection** — a view concern rather than a geometry
  one: `view.rs`'s `Section`, the viewport plane control, and `section` on
  `evaluate_part`, with the part untouched and every measurement still of the
  whole solid. OP_ROADMAP §8, docs/PERCEPTION.md §7.

What a mainstream tool has that this still does not, with the cost of each here,
is in [OP_ROADMAP.md](OP_ROADMAP.md).

### Still missing

| wanted | needed for | what it takes |
|---|---|---|
| **arcs in a section** | an O-ring groove, a bearing seat — a radius in section rather than a chamfer | `Edge::arc` is bound; the profile is a `Vec<[f64; 2]>` of straight segments, so an arc has nowhere to live |
| **helix** | real threads — every "threaded" hole in the corpus is drawn as its tap drill; a knurl; a spring | a helical path type. `MakePipe` is bound and `sweep()` uses it, but the graph's path model is runs and circular bends |
| **involute and other authored curves** | a spur gear, a cam, a real GT2 flank (`timing-pulley.js` approximates it and says so) | curve construction in the graph, on top of the section type |
| **re-entrant (non-convex) sections** | a stepped hub in one operation | refused deliberately: no exact distance field. A union of convex revolves is exact, and is how the part is turned anyway |
| **variable-radius and unequal-distance treatments** | a casting fillet that tapers, an asymmetric chamfer for a weld prep | `Fillet`/`Chamfer` take one scalar |
| **multi-body / assembly** | a pillow block *and* its bearing, any fit check | the graph has one root and one solid |

Deliberately absent and staying absent: anything that lets a part be
*approximately* right. A "thread" that is a stack of tori, or a cone faked from a
scaled cylinder, is the silent approximation the project refuses.

### What twenty-one real Fusion 360 designs actually needed

The list above came from parts *we* chose to model, which selects for what the
language can already do. Twenty-one Fusion documents from the reference corpus
were exported and measured instead; `examples/fusion360/` carries the ones worth
recreating, with the measured targets. Designs counted once each, not per use.

| feature | designs | parcad has it |
|---|---|---|
| `Sketch` | 19 | n/a — implicit in our ops |
| `Fillet` | 15 | yes |
| `ConstructionPlane` | 15 | n/a — no sketch planes |
| `Extrude` | 15 | yes |
| `CircularPattern` | 9 | yes (`polar`) |
| `Combine` | 7 | yes (booleans) |
| `Revolve` | 6 | yes |
| `Loft` | 6 | yes — B-rep only; the implicit backend refuses it by name |
| `Mirror` | 6 | yes (`.mirror()`) |
| `Sweep` | 6 | yes — B-rep only; a round profile is `pipe()`, exact in both |
| `Sphere` | 5 | yes |
| **`SplitBody`** | **5** | **no** |
| `Move` | 5 | yes (`.at()`) |
| `Shell` | 4 | yes |
| **`Thicken`** | **4** | **no** |
| **`Form`** (T-spline) | **4** | **no, and should stay no** |
| **`Remove`** (delete face) | **4** | **no** |
| `Pipe` | 3 | yes (`pipe()`) |
| **`Stitch`** (surfaces) | **3** | **no** |

**The wall was "parcad only makes analytic surfaces", and it has moved.** 13 of
the 21 have NURBS faces; in the worst (`v10`) it is 575 of 585. `Loft` and
`Sweep` tied at six designs each, each in more designs than `Revolve`, and that
count lifted the hold on both (OP_ROADMAP §3–4). What the counts could not say,
and the recreation targets did, is that the ops alone were not the wall: every
still-blocked target fails on *spline sketch geometry in the section or path*,
which the profile type cannot hold. (UnTriangle v3 looked blocked the same way
and was not: its "NURBS" walls probe as ruled patches, and the real obstacle was
loft's vertex pairing being silently normalised. Recreated — see
`examples/fusion360/README.md`.) `Thicken`, `Stitch` and `Patch` are the
surface-modelling side of that wall, out by decision.

**15 of the 21 are multi-solid**, which the one-root-one-solid graph cannot hold
at all. Some is assemblies proper and some is construction bodies that never get
combined, so the row is softer than 15/21 sounds — but it is not 1-in-20 either.

In proportion: one author, 21 documents, skewed toward decorative and 3D-printed
work rather than the machined fittings `examples/` covers. Evidence about
priorities, not a specification, and reproducible from
`reference/fusion/*/measurements.json` if that folder is present.

### What `Revolve` cost

**The implicit field is a bound outside a convex corner**, not an exact distance.
The exact nearest-segment formula was written first and rejected: under interval
arithmetic its clamped projection loses the correlation between its own terms,
the octree could no longer prove a cell empty, and a plain tube meshed to 31k
triangles at depth 6 instead of about 1k, with NaN vertices at depth 7. The
half-plane form is exact on the surface and inside and underestimates outside a
corner, which is safe for the mesher in the way an overestimate would not be.
`sdf::revolve` says so, and a unit test pins the under-read so that making it
exact later is a deliberate edit.

Also: **`role: "hole"` does not recognise a conical opening.** A countersink rim
is an inner boundary of the top face by any reading, and the term drops it, so
`cover-plate.js` selects on `curve` and position instead. Same cause as §6.

## 1. No polar array. Every round part rewrites the same loop — **FIXED**

`polar(count, radius, { straddle })` and `around(shape, count, axis)` sit beside
`grid()` in `dsl.ts`, with unit tests in `dsl.test.ts`. `grid()` gave rectangular
patterns only, so a bolt circle was authored arithmetic —
`((i + 0.5) / bolts) * Math.PI * 2` — repeated in `flange.js`,
`knurled-knob.js` and `timing-pulley.js` with only the count and radius
differing. That is where the standards knowledge hid: the `+ 0.5` is what makes
ASME bolt holes straddle the centrelines.

Two shapes were needed, not one. `polar()` returns *points* for `repeat()`, the
way `grid()` does; `around()` spins a whole *shape*, for a feature that is not
rotationally symmetric — a T-slot on each face of an extrusion, a flute around a
knob — and emits the first copy untransformed rather than a rotation by zero, so
it produces exactly the graph the hand-written version did.

Worth keeping: those two parts place cutters *exactly* on a surface, and moving
from `cos`/`sin` to a rotation changed their face and edge counts (72 faces to 61
on the knob) while leaving volume and area alone. The solid is the same; its
B-rep partitioning is not.

## 2. Selecting "the outermost" edges needs one selector per corner

`extrusion-2020.js` wants the four outside corners of the profile rounded. `"|Z"`
matches **37** edges, because every T-slot contributes edges running along the
extrusion too, and there is no way to say "of these, the ones on the outside", so
the script enumerates `[">X and >Y", ">X and <Y", "<X and >Y", "<X and <Y"]`.

**Fix:** either a `convex: true` / `role: "outer"` term in `EdgeQuery`, or allow
a directional selector to be scoped by provenance (`generatedBy` exists for
queries but the compact string form has no equivalent). The former is the more
useful: "break every convex edge" is a real manufacturing instruction and
currently inexpressible.

## 3. `adjacentTo: { faceNormal }` over-matches on cylindrical faces

A rim is adjacent to its own bore wall as well as to the flat face it sits in,
and a cylindrical wall answers to axis-aligned normals. On `manifold-block.js` a
bore drilled along **X** has both end rims matched by
`adjacentTo: { faceNormal: "+z" }` — the query meant to say "opens onto the top
face" picked up 8 edges instead of 6; on a plain block with one cross-drilling it
matches 2, both the +X/−X rims. `at: { z: "max" }` says what was meant and is
what the manifold, tee, pulley and knob use. The face-normal form is still
correct where every hole is drilled along one axis (`bracket.js`, `flange.js`,
`heat-sink.js`, `motor-mount.js`).

**Fix:** restrict the adjacency test to planar faces, or make the normal test
require the *whole* face to face that way rather than any sampled point. Decide
deliberately — a bore wall genuinely is adjacent, so the behaviour may be right
and the documentation wrong. Either way `EdgeQuery` should say which.

## 4. A blended union that cannot build refuses with a measured radius *(was: aborts the kernel)*

Three shapes used to crash OCCT with `SIGABRT` rather than returning a refusal,
all three hit while modelling ordinary parts:

| shape | first seen | radius that fails |
|---|---|---|
| two coaxial cylinders meeting exactly on a face | `flange.js`, hub on plate | every radius tried, including 1 mm |
| a union of four solids where two land exactly on the others' faces | `motor-mount.js`, gussets | 2 mm |
| two equal-radius cylinders crossing at 90° | `pipe-tee.js`, r = 21 | 2 mm and up; r = 16.7 is fine |

The workarounds are in the scripts, commented: bury the hub into the plate, blend
the two plates and union the gussets on afterwards, make the tee's run and branch
different diameters. All three are also what the real part looks like, so the
examples did not have to lie — but a part that *is* two coaxial cylinders has no
such escape.

**What landed** (after a field session bisected radii by hand, three evaluations
per number): the fillet boundary catches what OCCT raises
(`ParcadEdgeTreatment::build` in the vendored wrapper), so these arrive as
refusals, and the failure path probes below the failed radius — five bounded
bisection attempts, each held to build + containment + validity — so the message
names a radius *measured* to build, or says, measured, that none did. It also
reads the seam: the coaxial hub refuses naming the tangent face-on contact and
the overlap workaround; the equal tee refuses naming the 4-way seam junction at
the saddle and 1.13 mm as the largest radius that built.
`refuse-tangent-blend-union` and `refuse-unblendable-junction` pin both, and
`refuse-oversized-fillet` pins the same contract on the treatment path (5 mm
fails on the 10 mm cube; 4.85 mm is measured to build). Still open: the shapes
themselves refuse, and a probe that segfaults or spins ends as a crash or a
timeout naming the probe, which is what the worker process and its deadline are
for.

## 5. Four primitives go a long way — and where they stop (see §0)

Box, sphere, cylinder and — since §0 — a revolved section. The first three cover
a surprising amount alone: a hexagon is three intersecting slabs, a 90° vee is a
rotated cube, a triangular gusset is a cube cut by a rotated cube. Each is
*exact*, better than a sketch that has to be constrained, and adding a profile
type should not come at the cost of it. `intersect(slab, slab.rotate("z", 60),
slab.rotate("z", 120))` is a hexagon by construction and cannot be off; an
authored sketch needing a constraint solver is a different kind of object.

Where they stop is §0. The one worth repeating here is the sweep:
`timing-pulley.js` approximates a GT2 tooth with a cylinder per groove and is
marked APPROXIMATE in its first line, because burying that in the geometry would
be the silent approximation this project refuses.

## 6. `role: "hole"` means "a closed circle", which a D-bore is not

`knurled-knob.js` has a 6 mm bore with a flat. Its bottom outline is a straight
edge plus an arc, so `role: "hole"` matches **nothing** — a loud failure whose
reason is not obvious from the message. The knob selects
`{ generatedBy: "bored", at: { z: "min" } }` and expects 5, because the kernel
hands the arc back in pieces: a real assertion, but not a number anybody can
predict from the source.

**Fix:** `role: "hole"` should probably mean "an inner boundary of a face", which
a D-bore outline is, rather than "a full circle". At minimum the refusal should
distinguish "no edges matched because nothing was circular" from "no edges
matched at all".

## 7. Small things

- **`repeat()` takes points, not shapes.** Placing two *different* shapes at a
  list of positions falls back to `union(...list.map(...))`.
- **A tool that ends exactly on a face is a trap — partly FIXED.**
  `holeFor(thread, depth, { through })` does the overshoot itself, at both ends
  when the hole goes through. Round holes for named fasteners only; a slot or a
  pocket still oversizes by hand.
- **`grid()` returns `[x, y]`, but placements are `[x, y, z]`.** Lifting a
  pattern onto another plane is `.map(([x, z]) => [x, 0, z + h])`.
- **Every DSL export is a reserved word inside a script.** Scripts run as
  `new Function(...names, source)`, so `const hole = cylinder(3, 40)` collides
  with an export called `hole` and the script fails to parse. Adding `hole()` and
  `tapDrill()` broke four scripts in this repo at once, which is how the hazard
  was found: the cutter helper is called `holeFor` for that reason, and
  `hex-standoff.js` and `clevis.js` take their drill sizes from the table rather
  than shadowing it. The cost lands on parts already saved in someone's project
  folder, not only on the ones here. **Fix:** the places that compile a script
  (`tools/run.ts`, `app/src/engine.ts`, `app/src-tauri/src/script.rs`) should
  catch the redeclaration and name the builtin that was shadowed. "`hole` is a
  parcad builtin — rename your local" is a one-line fix for the reader; "Cannot
  declare a const variable twice" is not.

## 8. One Fusion feature costs a third of a part, because the language has no tool

`examples/fusion360/retainer-v1.js`, the strongest evidence here because both
sides can be counted. In Fusion the bayonet slot is three ordinary steps: sketch
a slot, extrude-cut it, chamfer the mouth. Here it is 49 of the file's 146 lines
— 36 for the slot and 13 for its lead-in chamfers, against 36 for the plate,
disc, blend, bore, thread and rim chamfer together (39 more are measured
constants, 22 the header). And the scaffolding leaks into the constants:
`TURN_CX`, `TURN_CY`, `POCKET_X_END` and `INNER_R` are not dimensions of the
part, they exist to decompose a shape by hand.

**A slot is a swept centreline, not an outline.** The sketch dimensions it the
way a machinist would — width 10.20, turn radius R12.00, and where the centreline
runs: a cutter of a given diameter following a path, whose round end is the
cutter's own end and whose corner radii are what a constant offset leaves. The
proof is in the file's own constants, `TURN_R = 12` and `INNER_R = 1.8`, carried
as two independent numbers when `12 − 1.8 = 10.2 = SLOT_W`. They were never two
dimensions: they are one centreline bend radius of 6.9 with the tool's half-width
added outside and subtracted inside, and the relationship was lost on the way in
because the language could not say "one bend radius".
`pipe(points, diameter, { bend })` is this idea one dimension up, exact for the
same reason: a straight run offset by a constant is straight, a circular bend
offset by a constant is circular.

**A finish is a later op, not part of the cutter.** The lead-ins are 45° reliefs
at the slot mouth, chamfered *after* the cut in Fusion; here they are triangular
prisms unioned into the cutter, so they must exist *before* it. The cause is §2
and §3 — the selector grammar reaches document extrema, so there is no term for
"the two vertical edges at the slot mouth" that does not also name the rest, and
that is why the corner radii are baked into cutters too.

The mechanisms that would make a part like this fast to write, ordered by how much
of the retainer each removes — not a plan, a list to pick from knowingly:

- **Say the tool, not the shape.** An end mill of diameter *D* along a path gives
  round ends, both corner radii and the bend relationship for free; ~36 lines
  here, and every future keyway, pocket lane and bayonet. `holeFor` is the
  precedent.
- **Let a dimension stay one dimension** — the 12/1.8 split. Structural, not
  authoring discipline: JS can compute `INNER_R = TURN_R - SLOT_W`, but nothing
  makes the author do it, and the recreation did not.
- **Name a feature, then refer to what it made.** `tag()` names a node; nothing
  names the edges or faces that node *created*. The general form of §2, §3 and
  the lead-ins.
- **Ops in the order you would perform them** — cut, then finish; currently
  forced by what can be selected instead.
- **Place against geometry, not coordinates.** Every position in the retainer is
  arithmetic: `.at(R_DISC, PLATE_L, H_DISC / 2)`.
- **A shop vocabulary, not a kernel one** — `slot`, `pocket`, `boss`, `rib`,
  `counterbore`, `keyway`, each one thing a machinist says.
- **A plane to work on.** `extrude` takes a 2D outline; siting it is 3D
  arithmetic.

Not asked for: a constraint solver — see OP_ROADMAP, "What is deliberately not on
this list". The gap is not that dimensions cannot be related, it is that the
*operations* are kernel-shaped rather than shop-shaped, so the author translates
before they can start relating them.

## What is genuinely good

Worth recording so nobody "fixes" it:

- **Shapes as values.** `heat-sink.js` places one fin nine times and the graph
  holds one node. Nothing had to be said to make that happen.
- **`.expect({ count })` earns its keep immediately.** Seven of the twelve parts
  had a selector matching a different number of edges than intended, and every
  one failed at the selector with a readable message rather than producing a
  quietly wrong solid.
- **Failures name the fix.** "expected 5 edge(s), but matched 4" plus the full
  query is enough to act on without opening a viewport.
- **Centred primitives placed by `.at()`** made the "bury the boss in the base"
  workarounds trivial.

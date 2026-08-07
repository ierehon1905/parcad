# DSL gaps

What writing `examples/` ran into. Every entry here is something the language
or the backend made harder than it should be, found while modelling twelve
standard mechanical parts (flange, pillow block, motor mount, manifold, tee,
pulley, extrusion, V-block, coupler, standoff, heat sink, knob).

Two kinds of entry, kept apart on purpose:

- **§0 is what the language cannot express at all.** Those parts were wanted
  and are not in `examples/` — not because they were hard to write, but because
  there is no operation that produces the shape. That is a feature list, not a
  workaround list.
- **§1 onward is friction**: things that model correctly but cost more than
  they should. All twelve parts in `examples/` build exactly and are measured
  in `eval/cases/`, so none of those are blockers.

Entries that have since been fixed are kept, marked **FIXED**, with what the
fix turned out to be — a gap that was closed is the most useful kind of record
when deciding whether the next one is real.

Format for all of them: what happens, where it hurts, and what would fix it.

---

## 0. Not expressible: the feature is missing

**Corrected once already.** This section originally said a cone was out of
reach because `opencascade-sys` binds no `BRepPrimAPI_MakeCone`. That was the
wrong layer to look at: a cone is a revolved triangle, and
`BRepPrimAPI_MakeRevol` *is* bound — as are `MakePrism` (extrude) and
`ThruSections` (loft), with `Edge`, `Wire` and `Face` to build a section from.
The kernel was never the constraint; our graph was. `Op::Revolve` now exists
because of it, and `cone()`, `countersink()` and any turned section came with
it for free.

The lesson is worth more than the op: before recording something as impossible,
check the layer that would implement it, not the layer above.

### Since fixed, by the same route

- **extrude an authored outline** — `Op::Extrude`, with `extrude(profile, h)`
  and `ngon(sides, size, h, { across })` in the DSL. Convex outlines only, the
  same rule and the same escape as a revolve section. `hex-prism` in
  `eval/cases/` checks it against the closed form for a hexagon.
- **mirror** — `Op::Mirror`, as `.mirror("x")`. This one needed a composition
  rather than a binding: `gp_Trsf::SetMirror` is bound only for an *axis*,
  which is a half turn about a line, not a reflection in a plane. The
  reflection is that half turn composed with a point inversion — both bound,
  both exact — and `mirrored-hand` in `eval/cases/` is a chiral part whose
  bounding box tells the two apart, because their volumes cannot.

A survey of what a mainstream tool has that this still does not, with the cost
of each here, is in [OP_ROADMAP.md](OP_ROADMAP.md).

### Still missing

| wanted | needed for | what it takes |
|---|---|---|
| **arcs in a section** | a true torus, an O-ring groove, a bearing seat — anything with a radius in section rather than a chamfer | `Edge::arc` is bound; the profile is a `Vec<[f64; 2]>` of straight segments, so an arc has nowhere to live yet |
| **helix** | real threads — every "threaded" hole in the corpus is drawn as its tap drill; a diamond knurl; a spring | a helical path type. `MakePipe` is bound now and `sweep()` uses it, but the graph's path model is runs and circular bends — a helix has nowhere to live |
| **involute and other authored curves** | a spur gear, a cam, a real GT2 flank (`timing-pulley.js` approximates it and says so) | curve construction in the graph, on top of the section type |
| **re-entrant (non-convex) sections** | a stepped hub in one operation | today it is refused, deliberately: no exact distance field. A union of convex revolves is exact, and is how the part is turned anyway |
| **variable-radius and unequal-distance treatments** | a casting fillet that tapers, an asymmetric chamfer for a weld prep | `Fillet`/`Chamfer` take one scalar |
| **section / cut-away for inspection** | seeing that `manifold-block.js`'s galleries meet without exporting an STL and slicing it elsewhere | a view concern, not a geometry one, and still the thing most missed while writing these |
| **multi-body / assembly** | a pillow block *and* its bearing, a tee *and* its pipes, any fit check | the graph has one root and one solid |

What is deliberately absent and should stay absent: anything that would let a
part be *approximately* right. A "thread" that is a stack of tori, or a cone
faked from a scaled cylinder, is exactly the silent approximation the project
refuses — `timing-pulley.js` is the boundary case, and it only exists because
it announces itself in the first line of the file.

### What twenty-one real Fusion 360 designs actually needed

The list above was derived from parts *we* chose to model, which selects for what
the language can already do. So twenty-one of the author's own Fusion documents
were exported and measured instead — a sample of one person's actual CAD rather
than of our imagination. `examples/fusion360/` carries the ones worth
recreating, with the measured targets; these are the counts across all 21.

How many designs use each feature (counted once per design, not per use):

| feature | designs | parcad has it |
|---|---|---|
| `Sketch` | 19 | n/a — implicit in our ops |
| `Fillet` | 15 | yes |
| `ConstructionPlane` | 15 | n/a — no sketch planes |
| `Extrude` | 15 | yes |
| `CircularPattern` | 9 | yes (`polar`) |
| `Combine` | 7 | yes (booleans) |
| `Revolve` | 6 | yes |
| **`Loft`** | **6** | yes — polygon sections, B-rep only; the implicit backend refuses it by name |
| `Mirror` | 6 | yes (`.mirror()`) |
| **`Sweep`** | **6** | yes — convex profile along runs and bends, B-rep only; round profile is `pipe()`, exact in both |
| `Sphere` | 5 | yes |
| **`SplitBody`** | **5** | **no** |
| `Move` | 5 | yes (`.at()`) |
| `Shell` | 4 | yes |
| **`Thicken`** | **4** | **no** |
| **`Form`** (T-spline) | **4** | **no, and should stay no** |
| **`Remove`** (delete face) | **4** | **no** |
| `Pipe` | 3 | no |
| **`Stitch`** (surfaces) | **3** | **no** |

Two conclusions, and the first is the one that matters:

**The wall was "parcad only makes analytic surfaces", and it has since
moved.** 13 of the 21 designs have NURBS faces; in the worst (`v10`) it is 575
of 585. `Loft` and `Sweep` were tied at six designs each — each used in more
designs than `Revolve`, which we did implement — and that count is what
eventually lifted the hold on both (docs/OP_ROADMAP.md §3–4: B-rep builds
them, the implicit backend refuses them by name rather than approximating a
field that probes would then trust). What the counts could not say, and the
recreation targets in `examples/fusion360/` did, is that the ops alone were
not the wall: every still-blocked target fails on *spline sketch geometry in
the section or path*, which the profile type cannot hold. (UnTriangle v3
looked blocked the same way and was not: probing its export showed the
"NURBS" walls are ruled patches, and the real obstacle was loft's vertex
pairing being silently normalised — recreated now, see
`examples/fusion360/README.md`.) Everything under
`Thicken`, `Stitch` and `Patch` remains the surface-modelling side of that
wall, out by decision.

**15 of the 21 are multi-solid**, which the one-root-one-solid graph cannot hold
at all. That is the "multi-body / assembly" row above, and this sample says it is
not a niche want — it is most real documents. Some of that is assemblies proper
and some is construction bodies that never get combined, so the row is softer
than 15/21 makes it sound, but it is not 1-in-20 either.

Worth keeping in proportion: one author, 21 documents, skewed toward decorative
and 3D-printed work rather than the machined fittings `examples/` covers. It is
evidence about priorities, not a specification. The counts are reproducible from
`reference/fusion/*/measurements.json` if that folder is present.

### What `Revolve` cost, and what it is worth reading about

Two findings from implementing it, both recorded in the code:

- **The implicit field is a bound outside a convex corner**, not an exact
  distance. The exact nearest-segment formula was written first and rejected:
  under interval arithmetic its clamped projection loses the correlation
  between its own terms, the octree could no longer prove a cell empty, and a
  plain tube meshed to 31k triangles at depth 6 instead of about 1k — with NaN
  vertices at depth 7. The half-plane form is exact on the surface and inside,
  underestimates outside a corner, and is safe for the mesher in the way an
  overestimate would not be. `sdf::revolve` says so, and a unit test pins the
  under-read so that making it exact later is a deliberate edit.
- **`role: "hole"` does not recognise a conical opening.** A countersink rim is
  an inner boundary of the top face by any reading, and the term drops it —
  `cover-plate.js` selects on `curve` and position instead. Same root cause as
  §6: the term means something narrower than it says.

## 1. No polar array. Every round part rewrites the same loop — **FIXED**

`polar(count, radius, { straddle })` and `around(shape, count, axis)` now sit
beside `grid()` in `dsl.ts`, with unit tests in `dsl.test.ts`. `flange.js` uses
the first, `knurled-knob.js`, `timing-pulley.js` and `extrusion-2020.js` the
second. The record of what was wrong with the old form:

`grid()` gave rectangular patterns only, so a bolt circle was authored
arithmetic:

```js
const boltHoles = Array.from({ length: bolts }, (_, i) => {
  const angle = ((i + 0.5) / bolts) * Math.PI * 2;
  return [Math.cos(angle) * (boltCircle / 2), Math.sin(angle) * (boltCircle / 2)];
});
```

That exact loop appears in `flange.js`, `knurled-knob.js` and
`timing-pulley.js`, with only the count and radius differing. It is also where
the standards knowledge hides — the `+ 0.5` is what makes ASME bolt holes
straddle the centrelines, and it is invisible arithmetic rather than an
authored intent.

Two shapes were needed, not one: `polar()` returns *points* for `repeat()`, the
way `grid()` does, and `around()` spins a whole *shape*, for a feature that is
not rotationally symmetric — a T-slot on each face of an extrusion, a flute
around a knob. `around()` emits the first copy untransformed rather than a
rotation by zero, so it produces exactly the graph the hand-written version did.

One thing the refactor showed, worth keeping in mind: `knurled-knob.js` and
`timing-pulley.js` place cutters *exactly* on a surface, and moving from
`cos`/`sin` arithmetic to a rotation changed their face and edge counts (72
faces to 61 on the knob) while leaving volume and area alone. The solid is the
same; its B-rep partitioning is not. Both cases say so in their `why`.

## 2. Selecting "the outermost" edges needs one selector per corner

`extrusion-2020.js` wants the four outside corners of the profile rounded.
`"|Z"` matches **37** edges, because every T-slot contributes edges running
along the extrusion too. There is no way to say "of these, the ones on the
outside", so the script enumerates:

```js
const corners = [">X and >Y", ">X and <Y", "<X and >Y", "<X and <Y"];
```

**Fix:** either a `convex: true` / `role: "outer"` term in `EdgeQuery`, or
allow a directional selector to be scoped by provenance (`generatedBy` already
exists for queries but the compact string form has no equivalent). The former
is the more useful one: "break every convex edge" is a real manufacturing
instruction and currently inexpressible.

## 3. `adjacentTo: { faceNormal }` over-matches on cylindrical faces

A rim is adjacent to its own bore wall as well as to the flat face it sits in,
and a cylindrical wall answers to axis-aligned normals. So on
`manifold-block.js`, a bore drilled along **X** has both of its end rims
matched by `adjacentTo: { faceNormal: "+z" }` — the query intended to mean
"opens onto the top face" picked up 8 edges instead of 6.

Reproduction, on a plain block with one cross-drilling:

```js
box(80, 50, 40)
  .cut(cylinder(4, 96).rotate("y", 90)).tag("drilled")
  .edges({ generatedBy: "drilled", curve: "circle", role: "hole",
           adjacentTo: { faceNormal: "+z" } })   // matches 2; both are +X/-X rims
```

`at: { z: "max" }` says what was meant and is what the manifold, tee, pulley
and knob examples use. The face-normal form is still correct on parts whose
holes are all drilled along one axis (`bracket.js`, `flange.js`, `heat-sink.js`,
`motor-mount.js`).

**Fix:** restrict the adjacency test to planar faces, or make the normal test
require the *whole* face to face that way rather than any sampled point. Worth
deciding deliberately — a bore wall genuinely is adjacent, so the current
behaviour may be right and the documentation wrong. Either way `EdgeQuery`
should say which.

## 4. A blended union aborts the kernel instead of refusing

Three separate shapes crash OCCT with `SIGABRT` rather than returning a
refusal, and all three were hit while modelling ordinary parts:

| shape | first seen | radius that fails |
|---|---|---|
| two coaxial cylinders meeting exactly on a face | `flange.js`, hub on plate | every radius tried, including 1 mm |
| a union of four solids where two land exactly on the others' faces | `motor-mount.js`, gussets | 2 mm |
| two equal-radius cylinders crossing at 90° | `pipe-tee.js`, r = 21 | 2 mm and up; r = 16.7 is fine |

The workarounds are in the scripts, commented: bury the hub into the plate,
blend the two plates and union the gussets on afterwards, make the tee's run
and branch different diameters. All three are also what the real part looks
like, so the examples did not have to lie — but a part that *is* two coaxial
cylinders has no such escape.

This one matters beyond convenience. "Refuse rather than approximate" is the
project's rule, and a crash is neither: `host.rs` catches it and reports an
honest breadcrumb, but the message says "this is usually a dimension the
operation cannot satisfy", which sends the reader looking for a radius problem
that is not there.

**Fix:** pre-check blended unions for coincident-face contact and either offset
the tool internally or `bail!` with the real reason. Failing that, extend the
crash message: name coincident faces as a known cause and point at the
overlap workaround.

## 5. Four primitives go a long way — and where they stop (see §0)

The primitives are box, sphere, cylinder and — since §0 — a revolved section.
The first three cover a surprising amount on their own —
a hexagon is three intersecting slabs, a 90° vee is a rotated cube, a triangular
gusset is a cube cut by a rotated cube — and each of those constructions is
*exact*, which is better than a sketch that has to be constrained.

What it does not cover is in §0, with the parts each absence blocks. The one
worth repeating here is the sweep: `timing-pulley.js` approximates a GT2 tooth
with a cylinder per groove and is marked APPROXIMATE in its first line, because
burying that in the geometry is precisely the silent approximation this project
refuses to make.

**The point of this entry** is the other half: three primitives went further
than expected, and the constructions above are exact rather than fitted. Adding
a profile type should not come at the cost of that — an authored sketch that
needs constraint solving is a different kind of object from `intersect(slab,
slab.rotate("z", 60), slab.rotate("z", 120))`, which is a hexagon by
construction and cannot be off.

## 6. `role: "hole"` means "a closed circle", which a D-bore is not

`knurled-knob.js` has a 6 mm bore with a flat. Its bottom outline is a straight
edge plus an arc, so `role: "hole"` matches **nothing** — and the failure is
loud but the reason is not obvious from the message.

The knob selects `{ generatedBy: "bored", at: { z: "min" } }` and expects 5,
because the kernel hands the arc back in pieces. The count is still a real
assertion, but "5" is not a number anybody can predict from the source.

**Fix:** `role: "hole"` should probably mean "an inner boundary of a face",
which a D-bore outline is, rather than "a full circle". At minimum, the refusal
message should distinguish "no edges matched because nothing was circular" from
"no edges matched at all".

## 7. Small things

- **`repeat()` takes points, not shapes.** Placing two *different* shapes at a
  list of positions falls back to `union(...list.map(...))`.
- **A tool that ends exactly on a face is a trap — partly FIXED.** Every example
  extends its cutters past the material for this reason, and every example had
  to say so in a comment. `holeFor(thread, depth, { through })` now does the
  overshoot itself, at both ends when the hole goes through. It only covers
  round holes for named fasteners; a slot or a pocket still oversizes by hand.

- **Every DSL export is a reserved word inside a script.** Scripts run as
  `new Function(...names, source)`, so `const hole = cylinder(3, 40)` in a part
  collides with an export called `hole` and the whole script fails to parse.
  Adding `hole()` and `tapDrill()` broke four scripts in this repo at once,
  which is how the hazard was found: the cutter helper is called `holeFor` for
  exactly that reason, and `hex-standoff.js` and `clevis.js` now take their
  drill sizes from the table rather than shadowing it.

  This is a real cost of every new export, and it lands on parts already saved
  in someone's project folder, not only on the ones here. **Fix:** the places
  that compile a script (`tools/run.ts`, `app/src/engine.ts`,
  `app/src-tauri/src/script.rs`) should catch the redeclaration and name the
  builtin that was shadowed. "`hole` is a parcad builtin — rename your local"
  is a one-line fix for the reader; "Cannot declare a const variable twice" is
  not.
- **`grid()` returns `[x, y]`, but placements are `[x, y, z]`.** Lifting a
  pattern onto another plane is `.map(([x, z]) => [x, 0, z + h])`, which reads
  badly at exactly the moment the reader is trying to picture the part.

## 8. One Fusion feature costs a third of a part, because the language has no tool

The evidence is `examples/fusion360/retainer-v1.js`, and it is the strongest in
this file because both sides can be counted. In Fusion the bayonet slot is three
ordinary steps: sketch a slot, extrude-cut it, chamfer the mouth. Here it is 49
of the file's 146 lines.

| what | lines | in Fusion |
|---|---|---|
| plate, disc, blend, bore, thread, rim chamfer | 36 | about the same number of steps |
| **the bayonet slot** | **36** | one sketch, one extrude-cut |
| **its lead-in chamfers** | **13** | one chamfer, applied after |
| measured constants | 39 | the sketch's dimensions |
| header | 22 | — |

And it is worse than a line count, because the scaffolding leaks into the
constants. `TURN_CX`, `TURN_CY`, `POCKET_X_END` and `INNER_R` are not dimensions
of the part; they exist to decompose a shape by hand.

**A slot is a swept centreline, not an outline.** The sketch dimensions it the
way a machinist would: width 10.20, turn radius R12.00, and where the centreline
runs. That is a cutter of a given diameter following a path — the round end is
the cutter's own end, and the corner radii are what a constant offset leaves.

The proof is in the file's own constants. It carries `TURN_R = 12` and
`INNER_R = 1.8` as two independent numbers, and

    12 − 1.8 = 10.2 = SLOT_W

They were never two dimensions. They are one centreline bend radius of 6.9 with
the tool's half-width added outside and subtracted inside. The language could
not say "one bend radius", so the author computed both halves and the
relationship — the thing that would keep them consistent when the slot width
changes — was lost on the way in. `pipe(points, diameter, { bend })` is this
exact idea one dimension up, and it is exact for the same reason: a straight run
offset by a constant is straight, a circular bend offset by a constant is
circular.

**A finish is a later op, not part of the cutter.** The lead-ins are 45° reliefs
at the slot mouth, chamfered *after* the cut in Fusion. Here they are triangular
prisms unioned into the cutter, so they must exist *before* it. That is not only
verbose, it is the wrong order: a finish has to be reasoned about as geometry.
The cause is §2 and §3 — the selector grammar reaches document extrema, so there
is no term for "the two vertical edges at the slot mouth" that does not also
name the rest. The same limitation is why the corner radii are baked into
cutters.

### The intuitive routes, and what each one buys

Not a plan — a list of the mechanisms that would make a part like this fast to
write, so the next piece of work can pick knowingly. Ordered by how much of the
retainer each one removes.

- **Say the tool, not the shape.** A slot is an end mill of diameter *D* along a
  path. Expressing the tool gives round ends, both corner radii and the bend
  relationship for free, and it is honest about manufacture. Removes ~36 lines
  here and every future keyway, pocket lane and bayonet. `holeFor` is the
  precedent that already works this way and nobody has complained about it.
- **Let a dimension stay one dimension.** The 12/1.8 split above. Where two
  numbers are functions of one, the API should take the one. This is structural,
  not authoring discipline — JS can compute `INNER_R = TURN_R - SLOT_W`, but
  nothing makes the author do it, and the recreation did not.
- **Name a feature, then refer to what it made.** `tag()` names a node; nothing
  names the edges or faces that node *created*. "Chamfer the edges this cut left
  on the top face" is the sentence the author wants and cannot write, and it is
  the general form of §2, §3 and the lead-ins here.
- **Ops in the order you would perform them.** Cut, then finish. The order is
  currently forced by what can be selected rather than by what is being made,
  which is how the lead-ins ended up inside a cutter.
- **Place against geometry, not coordinates.** Every position in the retainer is
  arithmetic: `.at(R_DISC, PLATE_L, H_DISC / 2)`. On a face, centred on a bore,
  flush with an edge — those are what the author means, and the arithmetic is
  the translation they are doing by hand.
- **A shop vocabulary, not a kernel one.** `slot`, `pocket`, `boss`, `rib`,
  `counterbore`, `keyway` each say one thing a machinist says. The kernel
  vocabulary is right for the graph and wrong for the part.
- **A plane to work on.** `extrude` takes a 2D outline, but siting it is 3D
  arithmetic. Sketching on a face is how the sketch above was made.

What this section deliberately does not ask for is a constraint solver. The
argument in OP_ROADMAP still holds — a script that says `gap / 2 + armT / 2` is
consistent by construction. The gap is not that dimensions cannot be related; it
is that the *operations* are kernel-shaped rather than shop-shaped, so the author
translates before they can even start relating them.

## What is genuinely good

Worth recording too, so nobody "fixes" it:

- **Shapes as values.** `heat-sink.js` places one fin nine times and the graph
  holds one node. Nothing had to be said to make that happen.
- **`.expect({ count })` earns its keep immediately.** Seven of the twelve parts
  had a selector that matched a different number of edges than intended, and
  every one of them failed at the selector with a readable message rather than
  producing a quietly wrong solid. That is the design working.
- **Failures name the fix.** "expected 5 edge(s), but matched 4" plus the full
  query is enough to act on without opening a viewport.
- **Centred primitives placed by `.at()`** stopped being annoying about three
  parts in, and made the "bury the boss in the base" workarounds trivial to
  express.

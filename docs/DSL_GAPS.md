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

### Still missing

| wanted | needed for | what it takes |
|---|---|---|
| **extrude an authored section** | a custom extrusion profile, a cam plate, any 2D outline given a thickness | a 2D section type, then `Op::Extrude` over the already-bound `MakePrism`. The section type is the real work, and `Revolve` has now defined what one looks like |
| **arcs in a section** | a true torus, an O-ring groove, a bearing seat — anything with a radius in section rather than a chamfer | `Edge::arc` is bound; the profile is a `Vec<[f64; 2]>` of straight segments, so an arc has nowhere to live yet |
| **helix** | real threads — every "threaded" hole in the corpus is drawn as its tap drill; a diamond knurl; a spring | a helical path plus a sweep. `MakePipe` is not currently bound |
| **involute and other authored curves** | a spur gear, a cam, a real GT2 flank (`timing-pulley.js` approximates it and says so) | curve construction in the graph, on top of the section type |
| **re-entrant (non-convex) sections** | a stepped hub in one operation | today it is refused, deliberately: no exact distance field. A union of convex revolves is exact, and is how the part is turned anyway |
| **mirror** | every symmetric part writes both halves by hand | `Op::Mirror`. It cannot be sugar over `Scale`, because non-uniform scale is refused, correctly |
| **variable-radius and unequal-distance treatments** | a casting fillet that tapers, an asymmetric chamfer for a weld prep | `Fillet`/`Chamfer` take one scalar |
| **section / cut-away for inspection** | seeing that `manifold-block.js`'s galleries meet without exporting an STL and slicing it elsewhere | a view concern, not a geometry one, and still the thing most missed while writing these |
| **multi-body / assembly** | a pillow block *and* its bearing, a tee *and* its pipes, any fit check | the graph has one root and one solid |

What is deliberately absent and should stay absent: anything that would let a
part be *approximately* right. A "thread" that is a stack of tori, or a cone
faked from a scaled cylinder, is exactly the silent approximation the project
refuses — `timing-pulley.js` is the boundary case, and it only exists because
it announces itself in the first line of the file.

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
- **A tool that ends exactly on a face is a trap.** Every example extends its
  cutters past the material for this reason, and every example has to say so in
  a comment. A `through()` helper that oversizes a cutter along its own axis
  would remove a whole class of comment.
- **`grid()` returns `[x, y]`, but placements are `[x, y, z]`.** Lifting a
  pattern onto another plane is `.map(([x, z]) => [x, 0, z + h])`, which reads
  badly at exactly the moment the reader is trying to picture the part.

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

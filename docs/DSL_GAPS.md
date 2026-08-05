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

Format for both: what happens, where it hurts, and what would fix it.

---

## 0. Not expressible: the feature is missing

The whole vocabulary is thirteen ops — `Cuboid`, `Sphere`, `Cylinder`, three
booleans, `Translate`, `Rotate`, `Scale`, `Offset`, `Shell`, `Fillet`,
`Chamfer` (`crates/parcad-core/src/graph.rs`). Everything below was reached for
while modelling the twelve example parts and abandoned, because no combination
of those thirteen produces it.

| wanted | needed for | missing op |
|---|---|---|
| **cone / tapered solid** | countersunk screw holes on `motor-mount.js` and `manifold-block.js`, any draft angle, a lathe centre, a chamfered boss | `Cone { r1, r2, h }` — the cheapest big win here; a countersink is the single most common feature in the whole corpus and none of the twelve has one |
| **torus** | an O-ring groove in `pipe-tee.js` and `manifold-block.js` port faces, a rounded rim without a fillet | `Torus { major, minor }` |
| **revolve a profile** | any turned part whose section is not a stack of cylinders: a proper flange hub taper, a bearing seat, a pulley crown | a 2D profile type, then `Revolve` |
| **sweep / extrude a profile along a path** | a true GT2 tooth (`timing-pulley.js` approximates it and says so), any custom extrusion section, a pipe bend, a cable channel | same 2D profile type, then `Sweep` |
| **helix** | real threads anywhere — every "threaded" hole in the corpus is drawn as its tap drill, and `hex-standoff.js` says so in a comment; a diamond knurl on `knurled-knob.js`; a spring | a helical path, which needs the sweep above |
| **involute / non-circular profile curves** | a spur gear, a cam, a proper GT2 flank | curve construction of any kind; today the only curves are the ones primitives happen to have |
| **mirror** | every symmetric part writes both halves by hand | `Mirror { axis }`. It cannot be sugar over `Scale`: non-uniform scale is refused, correctly |
| **variable-radius and unequal-distance treatments** | a casting fillet that tapers, an asymmetric chamfer for a weld prep | `Fillet`/`Chamfer` take one scalar |
| **section / cut-away for inspection** | seeing that `manifold-block.js`'s galleries actually meet without exporting an STL and slicing it elsewhere | a view concern, not a geometry one, but it is the thing most missed while writing these |
| **multi-body / assembly** | a pillow block *and* its bearing, a tee *and* its pipes, any fit check | the graph has one root and one solid |

Two of these compound: threads, knurls, springs and gear flanks are all
"sweep a profile along a path", so a profile type plus `Sweep` unlocks most of
the list. `Cone` and `Mirror` are small and independent, and would improve the
existing twelve parts today.

What is deliberately absent and should stay absent: anything that would let a
part be *approximately* right. A "thread" that is a stack of tori, or a cone
faked from a scaled cylinder, is exactly the silent approximation the project
refuses — `timing-pulley.js` is the boundary case, and it only exists because
it announces itself in the first line of the file.

## 1. No polar array. Every round part rewrites the same loop

`grid()` gives rectangular patterns; there is no rotational equivalent, so a
bolt circle is authored arithmetic:

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

**Fix:** `polar(count, radius, { offset })` beside `grid()`, returning the same
`[x, y]` tuple list. Ten lines in `dsl.ts`, and three examples get shorter.

A second form is worth considering: `pattern(shape, "z", count)` that rotates
copies rather than translating them, for slots that are not on a circle of
points — `extrusion-2020.js` writes its four T-slots as four explicit
`.rotate("z", n * 90)` calls.

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

## 5. Three primitives go a long way — and where they stop (see §0)

The primitives are box, sphere and cylinder. That covers a surprising amount —
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

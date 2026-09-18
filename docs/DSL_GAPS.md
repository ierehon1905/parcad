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
- **a helix, and a taper along a sweep** — `pipe({ helix: { radius, pitch,
  turns, endRadius, hand } }, d, { taper })` and the same path and option on
  `sweep(profile, …)`. Asked for by a model building a unicorn over MCP, which
  faked a mane, a tail and a spiral horn out of stacked primitives. The helix is
  a line in a cylinder's (or, with `endRadius`, a cone's) parameter space, its
  fitted 3D curve measured within 1e-8 mm of the exact helix and refused past
  1e-4; the sweep is `BRepOffsetAPI_MakePipeShell` in Frenet mode, which on a
  helix is its screw motion. Five cases hold it to closed forms at 1e-6 of
  BRepGProp — `helical-spring`, `square-coil`, `left-hand-hook`,
  `tapered-strand`, `spiral-horn` — and `refuse-coil-through-itself` holds the
  pitch check. The taper is linear in *length* along runs and bends but in *turn angle* on a
  helix, measured: `spiral-horn` reads 811.62 mm³, where length would give
  614.81.

  A helical groove cut through the cylinder it wraps was recorded here as
  opening past two turns. That was parcad's seam-pcurve pass damaging the
  cylinder's side, not the boolean, and a guard added on main for another part
  had already fixed it when the helix landed (docs/GOTCHAS.md, "A helix cut
  through its own cylinder"); `helical-groove` holds it to a closed form.
- **a screw thread** — `threadedRod(size, length, { clearance, hand, pitch })`
  and `threadedHole(size, depth, { through, clearance, hand, pitch })`, lowered
  to `Op::Thread`: the ISO 68-1 basic 60° profile, a size from
  `METRIC_FASTENERS` (which now carries the ISO 261 coarse pitch) or
  `{ diameter, pitch }` for anything else of that profile — a 1/4"-20 tripod
  screw, an M40 × 3 jar neck. The tooth is swept in the axial plane along a
  helix of one edge per turn, unioned with a core of exactly the swept height
  and squared off by cutting two boxes; every build is measured against the
  slab closed form and refused past 2e-5. Five cases hold it — M8 at 3, 8 and
  20 turns in both hands, a bolt and nut pair at a 0.2 mm clearance read back
  as 0.200 between them, and the seeded `examples/screw-top-jar.js`. Not
  here: rounded roots (ISO's d3), tolerance classes (6g/6H are a clearance
  here), end chamfers or a thread run-out, tapered pipe threads, and any
  profile but 60°. `holeFor(size, depth, { tapped: true })` stays the drawing
  of a hole a machinist will tap.
- **several bodies that stay several** — `return { base, lid }`, an object of
  named shapes in place of one, lowered to a root-only `Op::Bodies`. The cheap
  end of the multi-body row, and deliberately only that: the bodies are built,
  measured and exported together and never fused, each is measured alone
  (`named_bodies`, with its own `pieces` count for the accidental split the
  part-level `bodies` could not name), and every pair is measured on the exact
  solids (`between_bodies`: the `check_fit` verdict, clearance and shared
  volume). STEP writes a solid per body; STL one file, or one body by name.
  `examples/lidded-box.js` is the seeded one; `split-halves` in the corpus is
  the two printable halves §9 asked for, held to the closed form of the kerf.
  Not here, by decision: joints, mates, constraints, assembly hierarchy,
  instancing across parts, motion — a body sits where its script put it, and
  the fit between two is measured, not solved. Nothing selects across bodies:
  a tag lives in the
  body that made it, and a treatment cannot take the group as its child.
- **a thin wall lofted through fitted sections** — `loft(sections, { wall: t })`
  for sections that are each one `{ fit }`: the kernel makes the inside itself,
  stepped `t` along the outside's surface normal at any lean, on the
  outside's parameters, and reports the wall it measured between the two skins
  as `loft_wall_mm`; `bottom`/`top: "closed"` give an end a floor. A lampshade
  used to be a loft minus a loft of hand-stepped points, which pinched to
  0.001 mm between smooth sections and was 1.21 mm thick for 1.6 asked
  between ruled ones. `walled-cone`, `walled-sleeve`, `walled-bowl`,
  `walled-dome` and `walled-shallow-shade` hold it to closed forms, the last
  three at 9–11° off flat. Not here: a wall on corner or arc sections (cut
  `loft` of `inset`s instead), or a wall whose profile bends tighter than
  its thickness.

What a mainstream tool has that this still does not, with the cost of each here,
is in [OP_ROADMAP.md](OP_ROADMAP.md).

- **arcs and splines in a section, re-entrant sections, spline paths** —
  one section type for `extrude`, `revolve`, `loft` and `sweep`: corners, `{ at,
  round }`, `{ through }` and `{ radius }` arcs, `{ spline }`, `{ bezier }` and
  `{ bspline }` curves, a loft that ends on `{ z, point }`, and `{ spline }` as a
  `pipe`/`sweep` path. Resolved in `parcad-core/src/section.rs` into exact
  arcs and B-spline poles, not polygons; OP_ROADMAP §2 has the prior art, the
  twelve closed forms and what it did and did not move. A re-entrant section
  builds, because the refusal's reason ("no exact distance field", then
  "vertex pairing is a silent guess") went with the implicit kernel and the
  loft's compatibility pass; `re-entrant-loft` holds the pairing to a frustum's
  closed form. `examples/hydraulic-line.js` draws its gland as the catalogue
  section now.

- **curves given by a formula, and involute spur gears** —
  `{ curve: (t) => [x, y], from, to, tolerance }` in any section, and
  `spurGearOutline({ module, teeth })` built on it. The kernel cannot run a
  script's function, so the script draws it: cubic Hermite pieces on the
  function's own points and directions, halved until each is within the
  tolerance, sent as a C1 B-spline with the bound beside it. Given the exact
  `derivative` and a bound `fourth` on the fourth derivative the bound is
  *certified* (the Hermite remainder, √2·m·h⁴/384 a piece); a bare function
  gets an *estimated* one, read off the function between the pieces, and the
  report says which as `curve_bound`. The kernel measures the built curve
  against points of the function the pieces were not drawn through
  (`deviation_mm`) and refuses a graph whose curve contradicts its bound.
  Module 2, 20 teeth, 20°: every flank certified within 7.1e-6 mm in eight
  pieces, read back from the exported STEP 2.9e-6 mm from the analytic
  involute; the routes it replaces were a `spline` through ten samples
  (1.8e-3 mm, built, nothing stated), a `fit` through twenty (4.5e-4 mm,
  `deviation_mm` against the samples only), an arc per flank (0.037 mm) and
  the pulley's cylindrical grooves (0.8 mm). `spur-gears` holds a meshed pair
  to the 0.094 mm flank gap 0.1 mm of backlash predicts; `involute-gear`
  holds a flank to points on the involute and 0.01 mm either side.

- **profile-shifted gears, and a pair that meshes** — `profileShift` on
  `spurGearOutline` moves tip and root out by x·module and thickens the tooth
  by 2x·module·tan α, which is how a pinion under 17 teeth avoids undercut;
  the undercut refusal names the least shift, `1 − (z/2)·sin²α`.
  `spurGearPair({ module, teeth: [z1, z2], profileShift, backlash })` returns
  both outlines, the centre distance from the working pressure angle
  (`inv αw = inv α + 2 tan α (x1 + x2)/(z1 + z2)`), the tip shortening that
  keeps 0.25·module of root clearance, and the turn that faces a space to a
  tooth — `180/z2` only for an even count, which the old advice got wrong for
  odd ones. It refuses a pair that jams below a base circle or whose contact
  ratio is under 1. `shifted-pinion` holds a 12-tooth, x = 0.3 pinion's tip,
  root, base-circle tangency and reference-circle thickness to the formulas;
  `shifted-gear-pair` and its turned twin read the 0.050 mm backlash/2 gap at
  the 42.572 mm centres a shifted 12/30 pair needs. A simulated hob — a basic
  rack with a 0.38·module tip radius rolled through a blank in 0.25° steps —
  leaves that pinion's whole flank on its surface to 0.1 µm and the radial
  line below the base circle up to 0.39 mm inside its material, so the
  outline keeps nothing a hob removes; unshifted, the same hob cuts the
  12-tooth flank away from the base circle up to r = 11.33 mm, which is what
  the refusal is for.

### Still missing

| wanted | needed for | what it takes |
|---|---|---|
| **thread forms past the basic 60° profile** | a trapezoidal lead screw, a buttress or bottle-cap thread, a tapered pipe thread, a rounded root | `Op::Thread` sweeps one trapezoid; another profile is another tooth and its own closed form, a taper a conical core and helix |
| **gear forms past the shifted spur** | an undercut or fillet-rooted hobbed gear, a helical, internal or bevel gear, a rack; a real GT2 flank (`timing-pulley.js` approximates it and says so) | the involute and the profile shift are drawn and certified (above). Below the base circle `spurGearOutline` runs the flank straight in, inside the trochoid fillet a hob leaves (measured), so a strength-critical root is thinner than the part a hob cuts; an undercut gear is refused, because its flank is the trochoid of the rack's tip radius — an offset of an extended involute, certifiable like the involute, meeting the involute at a point found by root-finding. A rack and an internal gear are other `{ curve }` entries with their own closed forms; a helical gear is a twisted loft or sweep of the outline; GT2 is missing its numbers, not a curve type |
| **draft on a curved or re-entrant outline** | a moulded boss with rounded corners in one op | the drafted top is a half-plane inset, which only a convex polygon has; draft the polygon and fillet its vertical edges |
| **variable-radius and unequal-distance treatments** | a casting fillet that tapers, an asymmetric chamfer for a weld prep | `Fillet`/`Chamfer` take one scalar |
| **assembly: joints, mates, constraints** | a pillow block *and* its bearing, placed by a fit rather than by coordinates | the bodies exist (above) and the fit between them is measured; nothing yet *places* one against another — a solver, which is the wide reading of NEXT.md §2 |

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
| `Loft` | 6 | yes |
| `Mirror` | 6 | yes (`.mirror()`) |
| `Sweep` | 6 | yes — a round profile is `pipe()` |
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
which the profile type could not hold — until it could, and one of the four
moved (`untitled2-v1`); the probe showed the other three were blocked on a
different fit, an ornament and a mismatched export (eval/targets/fusion360/README.md).
(UnTriangle v3 looked blocked the same way
and was not: its "NURBS" walls probe as ruled patches, and the real obstacle was
loft's vertex pairing being silently normalised. Recreated — see
`examples/fusion360/README.md`.) `Thicken`, `Stitch` and `Patch` are the
surface-modelling side of that wall, out by decision.

**15 of the 21 are multi-solid.** Some is assemblies proper and some is
construction bodies that never get combined, so the row is softer than 15/21
sounds — but it is not 1-in-20 either. The graph can now hold several solids
(`return { base, lid }`, above); what it cannot do is the assembly half, where
a body is *placed* by a joint rather than by its own coordinates.

In proportion: one author, 21 documents, skewed toward decorative and 3D-printed
work rather than the machined fittings `examples/` covers. Evidence about
priorities, not a specification, and reproducible from
`reference/fusion/*/measurements.json` if that folder is present.

### What `Revolve` cost

**`role: "hole"` does not recognise a conical opening.** A countersink rim
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

**Landed (2026-09-12):** `edges({ dihedral: "convex", parallel: "z" })` says
it, and `{ on: "profile", dihedral: "convex", parallel: "z" }` scopes it to
one feature's faces; [SELECTORS.md](SELECTORS.md).

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
  (`tools/run.ts`, `app/src/engine.ts`, `crates/parcad-host/src/script.rs`) should
  catch the redeclaration and name the builtin that was shadowed. "`hole` is a
  parcad builtin — rename your local" is a one-line fix for the reader; "Cannot
  declare a const variable twice" is not.

  **Second instance, 2026-09-18.** The coin-holder session
  (docs/COIN_HOLDER_REVIEW.md, L1) wrote `const clearance = 0.6` on its first
  build, eleven calls after reading the rule in read_docs `dsl`, and got
  QuickJS's "invalid redefinition of parameter name", which names nothing; it
  went to read_docs about an unrelated function and resent the part. The fix
  above was specified here and never built. **Built, the same day:**
  `__parcadShadowedBuiltin` in `dsl.ts` proves which name collided by
  recompiling without each declared builtin (never a regex guess), and all
  three compilers say "`clearance` is one of the N names parcad puts in every
  script, so it cannot be declared again; rename it". Measured by
  `eval/field/which-name-is-taken.md`.

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

## 9. What a laptop holder cost (2026-09-12)

A VESA-mounted V tray for a 16" MacBook Pro, built by a model over the CLI in
one session: a disc hub with the 75 and 100 patterns, two tapered arms, an
L-shaped lip at each front corner carved by cutting the laptop's own shape out
of a block, split at the centreline for a 256 mm bed. It builds, measured. The
part is `v-holder.parcad` in the project folder of the machine it was made on.
What it cost, worst first, each one reproduced on a small shape before being
recorded here:

- **Selecting edges is the structural problem.** Every failed evaluation but one
  was a selector reaching more than the intent named, and the language has no
  way to name less than the whole solid after a boolean. Argued in
  [SELECTORS.md](SELECTORS.md) rather than here; the short form is that queries
  need a scope, edges need to know their dihedral angle, and errors need to name
  the edge. All three landed the same day: `dihedral`, `parallel`,
  `longerThan`, smooth edges skipped by treatments, failures listing their
  edges, and then `on` and `between` over faces that keep their tag through
  booleans, treatments and rigid motions, with `at` measured within the
  feature. What remains is in [SELECTORS.md](SELECTORS.md) §3.
- **`.offset()` of a filleted body was unusable by any later boolean — FIXED.**
  The natural way to wrap a real object is to grow it by the clearance and cut
  it out: `holder.cut(laptop.offset(1))`. With a fillet anywhere on the body
  the cut, and a union, returned "the result has no faces", which blamed the
  boolean. The offset alone measured right, so the bounding-box post-condition
  could not see it: the thick-solid builder returns the grown body inside out.
  The lowering now closes the grown body into a solid and turns it outward
  (`facing_outward`, docs/VALIDITY_CHECKS.md); `eval/cases/offset-filleted-cutter` holds
  the pocket against its closed form. GOTCHAS, "`offset_surface` lies".
- **A cut that removed nothing was silent — FIXED.** Four VESA 100 holes fell
  outside a Ø120 hub and vanished; the face count said so one evaluation later.
  A subtract that makes no edge and takes no face now refuses, naming both
  extents (`refuse-missed-cut`).
- **An empty intersection was an error, not a zero — FIXED, twice.** The
  refusal now says 0 mm³ with both extents (`refuse-empty-intersection`), and
  the question it stood in for has its own tool: `check_fit` (MCP), `--fit`
  (CLI) lays a reference body against the part on the exact solids and reports
  `clear`, `touching` or `interfering`, the shared volume, and the clearance
  with the two points it is measured between. The V holder at rest: touching;
  lifted 0.5 mm: clear by 0.500; sunk 1 mm: 27 653 mm³ shared, which is the
  floor area the report already gave, times one. A second solid that is
  measured against and never joined is exactly what this is.
- **`sweep` refused every bent sheet — FIXED.** A 200 × 3 strip could not bend
  at 6 mm because `Op::sweep_spine` measured the profile's reach as the hypot
  of every point, 100 mm. On a planar path the profile keeps one axis in the
  plane, so only its extent toward the inside of the bend is in the way: the
  check now measures that, in the backend's own frame, and falls back to the
  full reach off-plane. `bent-sheet` measures the strip against Pappus to
  4 mm³ in 141 000.
- **Every position is arithmetic about geometry that already exists.** The
  chevron's point is placed by iterating where two arm centrelines must meet
  for their inner edges to cross at a chosen spot; the cups are placed from a
  unit vector, its normal, a point on the arm's outer edge and where that edge
  crosses the laptop's side. About 25 lines of the script derive points that
  are already on some shape, and two of the session's four authoring mistakes
  were in those lines. Wanted: points and lines *from* entities — a corner of
  the laptop, an edge of the arm as a line, `pointAt`, `meet`, `offset` — and
  placement relative to one; `hull(points)` for a convex outline. The
  shape-side half of what Fusion does with a sketch on a face, which the
  reference corpus used in 15 of 21 designs. §8's "place against geometry, not
  coordinates", with the cost measured.

  **Landed, the arithmetic half:** `line2d(from, to)` with `pointAt`,
  `offset`, `meet`, `yAt`/`xAt`, and `hull(points)` for a convex outline that
  `extrude` accepts by construction. The V holder's fan is now five named
  points read off two lines, and measures the same 302 406 mm³. **Not landed,
  and not landable as the language stands:** a line taken *from a built
  edge*. A script runs before the kernel does, so nothing in it can ask where
  an edge ended up; `list_entities` and `check_fit` are the measured route,
  one evaluation later. A sketch on a face would need the graph itself to
  carry construction geometry, which is OP_ROADMAP's "construction plane" row.
- **Real objects were guesses — FIXED, as far as a table can fix it.** The
  laptop's plan corner radius and bottom edge radius were assumed at 12 and
  5 mm in a script. `DEVICES` in `dsl.ts` now holds four MacBooks with their
  published sizes and those radii, labelled as read off photographs;
  `device(name, { clearance })` is the grown body a holder cuts out of itself,
  and `vesaPattern(size)` the four points. The V holder reads the table and
  measures the same 302 406 mm³ it did with the literals. The radii are still
  the honest weak point: a caliper on one machine would settle them for
  everyone.
- **Two printable halves need a flag and two evaluations — FIXED**, from the
  cheap end of the multi-body row of §0: `return { left, right: left.mirror("x") }`
  is one evaluation, measured per half and between them.
  `eval/cases/split-halves.json` holds exactly that shape — a bored block split
  with a 0.5 mm kerf — to the closed form of each half and to the kerf as the
  measured clearance.

Not the tool's fault, and worth keeping: the hub was drawn too small for the
100 pattern, and the report's "stands on … 1 patch" line plus the face count
caught it without a picture. Both drafts were checked at both ends of the
bounding box before delivery.

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

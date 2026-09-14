# Op coverage, measured against Fusion 360

What a mainstream parametric CAD tool offers, what parcad has, and what the
missing ones would cost *here*. Fusion 360 is the yardstick because it is the
tool most people who would use this have used; the comparison is about the
**operations**, not the interaction model.

The deciding rule is the project's, not Fusion's:

> **An op ships when the kernel can be honest about it, and a closed form can
> check it.** OCCT builds far more than it can be trusted with. Anything whose
> result cannot be measured against something independent — a closed-form
> volume, a containment box, a fitted curve's deviation — gets refused,
> restricted to the case that can, or left out, never approximated.

That rule is why this list is short rather than a wish list, and why some entries
are marked *hold* with a reason instead of a plan.

---

## Where we stand

| Fusion 360 | parcad | note |
|---|---|---|
| Box / Sphere / Cylinder primitives | ✅ `box` `sphere` `cylinder` | centred on the origin, placed with `.at()` |
| Extrude a sketch profile | ✅ `extrude`, `ngon` | convex outlines; see *Draft* and *Arcs* |
| Revolve a sketch profile | ✅ `revolve`, `cone`, `countersink` | convex sections |
| Combine (join / cut / intersect) | ✅ `union` `cut` `intersect` | plus `blend` for a rounded seam |
| Move / Copy, Rotate, Scale | ✅ `.at()` `.rotate()` `.scale()` | non-uniform scale builds exact B-splines (`BRepBuilderAPI_GTransform`), held to the determinant |
| Mirror | ✅ `.mirror()` | |
| Rectangular / circular pattern | ✅ `grid` `polar` `around` `repeat` | DSL helpers, not graph ops — the graph shares one node |
| Fillet / Chamfer | ✅ `.fillet()` `.chamfer()` with selectors | constant radius / equal distance |
| Torus primitive | ✅ `torus(major, minor, { sweep })` | full ring or arc; the arc is what a bend is |
| Shell | ✅ `.shell()` | |
| Offset face / Thicken | ✅ `.offset()` | whole-body offset, not per face |
| Hole | ✅ `holeFor` `tapDrill` `clearance` `counterbore` | ISO metric coarse, M2–M20 |
| Draft | ✅ `extrude(..., { draft })` | §1 |
| Sweep | ✅ `sweep(profile, path, { bend, taper })`, `pipe(path, dia, { bend, taper })` | runs and bend arcs or a `{ helix }`; no spline path |
| Loft | ✅ `loft(sections, { smooth })` | §4 |
| Section view | ✅ viewport plane, `section` on `evaluate_part` | §8 |
| Coil | ✅ `pipe({ helix }, dia)`, `sweep(profile, { helix })` | §5 |
| **Thread** | ❌ | the helix exists, the thread cut does not close — §5 |
| **Rib / Web** | ❌ | sugar over what exists — §6 |
| **Split body / face** | ❌ | a different request from the section view; §8 |
| Sheet metal, Surface/T-spline, Mesh, Simulation, CAM | ❌ | out of scope by design |
| Sketch constraints, timeline, parameters | n/a | the DSL is the parametric model; JS is a better parameter table |
| Several bodies in one part | ✅ `return { base, lid }` | never fused; each measured alone and every pair measured on the exact solids; STEP a solid per body; B-rep only |
| Assemblies / joints | ❌ | a body is placed by its own coordinates, never by a mate — a solver, and the wide reading of NEXT.md §3 |

---

## 1. Draft — **DONE**

`extrude(profile, height, { draft })` and `ngon(..., { draft })`; `revolve` does
not take it. An angle the outline cannot carry is refused, naming the maximum.
`drafted-boss` in `eval/cases/` holds the kernel to the closed form for a
square frustum (29282.008 mm³); `refuse-impossible-draft` holds the refusal.

## 2. Arcs in a section — the enabling change

An arc in the section is what makes an O-ring groove, a bearing seat, a radiused
shoulder. It is also what makes `Op::Fillet` unnecessary for turned work: a
radius authored in section is exact, where a rolling-ball fillet on the solid is
a surface fit.

**What it takes.** The profile type stops being `Vec<[f64; 2]>` and becomes a
list of segments and arcs. Convexity still decides: a convex arc bulging outward
has an exact field (distance to the centre, minus the radius), and the half-plane
max extends to it unchanged. `Edge::arc` is already bound, so the B-rep side is a
different `Edge` constructor in the same loop. **Cost:** medium.

A **torus primitive** was the cheap down payment and shipped:
`torus(major, minor, { sweep })`, one periodic face, an arc of it being a
pipe bend. `eval/cases/torus.json` checks volume *and* area against the closed
form; `eval/cases/torus-gland.json` checks the O-ring groove that used to
segfault in `UnifySameDomain` before OCCT 8.0.1 was vendored (docs/GOTCHAS.md).

**What the torus still cannot be is a catalogue gland.** That section is
rectangular and wider than the cord; a torus cut is a circle. The round bottom in
`examples/hydraulic-line.js` is the honest shape, and the rest waits on the
profile type — which is the argument for it, made by a part rather than a table.

## 3. Sweep — **DONE**, in two honesty classes

A path is made of straight runs and circular bends — what a tube bender does and
what CAM posts — and that is what both forms take.

`pipe(points, diameter, { bend })` is a union of cylinders and partial tori,
every one of them exact. Without a bend
radius the corners are square and filled with a ball — inside the swept envelope,
fine for clearance, not a shape anybody can make; with one, a bend that will not
fit refuses, naming the largest radius the corner takes
(`refuse-tight-sweep-bend`). `bent-tube` checks a
90° bend against the closed form (π·25·70 + π·15·25·π/2 = 7348.34 mm³), and
`pipe-run` checks the square-cornered form against a closed form that is *not*
the obvious one — two perpendicular runs overlap in a quarter of a Steinmetz
solid, and the ball that fills the corner is three quarters redundant.

`sweep(profile, path, { bend })` takes a convex outline along the same path
model. `swept-channel` holds it against Pappus's closed form (9141.59 mm³,
read 0.005% under by tessellation, the same effect `bent-tube` records). A
*round* profile should stay a `pipe()`.

**Since added:** a helical path and a linear taper (§5).

**Still out:** a spline path. There is no spline type in the graph to sweep
along, which is the section-and-path authoring gap that also blocks the Fusion
targets; a profile that twists along the path is out with it.

## 4. Loft — **DONE**, and the hold lifted deliberately

The hold this entry once recorded was lifted as a product decision — loft and
sweep each appear in more of the owner's real documents than revolve, which we
did build (DSL_GAPS, "What twenty-one real Fusion 360 designs actually needed")
— and the way it was lifted matters more than the op:

**Refuse rather than approximate, even here.** While the implicit backend
existed it refused a loft by name rather than fit a distance field to one — an
approximate field would have meant probes and wall thickness confidently
measuring a part that does not exist — and the cost was that a lofted part lost
every field-backed capability at once. Both went with the field: probes, wall
thickness, renders and sections run on the exact solid, so a loft is inspected
like any other op. `loft-frustum` holds the closed form (a 40→20 mm square
prismatoid, 28000 mm³ exactly).

`loft(sections, { smooth })` takes two or more convex polygon outlines stacked
along +Z. Sections must share a point count, because vertex pairing is by outline
index and taken literally: a rotated outline authors a *twisted* wall on purpose,
which is what recreated UnTriangle v3, held to a closed form by
`eval/cases/twisted-loft.json`. `smooth: true` is Fusion's look, one surface
fitted through all sections, and a fit that bulges past the sections' own
bounding box by more than the slip tolerance is refused
(`eval/cases/refuse-bulging-loft.json`), so the graph's cheap bounds stay honest.
Sections stay convex for the extrude/revolve reason plus loft's own: on a
re-entrant outline the kernel's vertex pairing is a silent guess.

## 5. Coils — **DONE**; threads — still held, now by a measurement

The hold was lifted by a model building a unicorn, which needed a spiral horn,
a tapering mane and a tail and faked them from stacked primitives. What shipped
is the honest version: `{ helix: { radius, pitch, turns, endRadius, hand } }`
as a path for `pipe` and `sweep`, and `taper` on both. The helix is a straight
line in a cylinder's or cone's parameter space; its fitted 3D curve is
*measured* against the exact helix (1e-8 mm on the corpus) and refused past
1e-4 mm. Five eval cases hold volume to closed forms at about 1e-6 of
BRepGProp, and the graph refuses a pitch that runs a turn into the next and a
section that crosses the axis at either end.

A **thread** is still not here, and the reason is no longer the op. A groove
swept along a helix and cut from a cylinder on the same axis builds at one and
two turns and returns an open surface at three or more, which the watertight
backstop refuses (docs/DSL_GAPS.md has the counts). Faking one from tori stays
refused for the old reason. So the **thread annotation on the node** — a
`PartReport` that says "M6 × 1, 12 deep" for a hole whose geometry is honestly
a 5 mm drill — is still the more useful next step for machined parts, and the
boolean is what to diagnose before a modelled thread.

## 6. Rib / Web — sugar, not an op

Fusion's Rib grows a thin wall from a sketch line down to the body. Here that is
`union(body, box(...).at(...))`, and the gussets in `motor-mount.js` and
`pillow-block.js` already do it. A `rib(from, to, thickness)` helper in `dsl.ts`
would remove the arithmetic, the way `polar()` did. No graph change.

## 7. Hole feature — **DONE**

`METRIC_FASTENERS` (M2–M20), with `tapDrill`, `clearance(thread, fit)`,
`counterbore` and `countersink("M5")` reading from it, plus
`holeFor(thread, depth, { fit, tapped, through })` for the cutter: DSL_GAPS §1's
"standards knowledge hides in arithmetic" applied to the numbers a machinist
knows by heart. Never write one as a literal in a part.

The cutter is called `holeFor` because an export called `hole` broke four saved
scripts that had `const hole = cylinder(...)` — every export becomes a parameter
name in the script sandbox. That hazard is DSL_GAPS §7 and binds every future
export.

## 8. Split and section — a view concern — done

Splitting a body for inspection is a clipping plane, not an op: the section
control on the viewport, and `section` on `evaluate_part` for a model that has no
window. The part is untouched — every measurement in the reply is still of the
whole solid. docs/PERCEPTION.md §7 records how it works and the two ways it can
quietly cut nothing.

**Splitting a body into two *parts*** is two intersections returned as two
bodies — `return { left: part.intersect(box(...)), right: ... }` — measured
per half and against each other; `eval/cases/split-halves.json` is that shape.
There is no one-call split op, and a mirrored half is the usual second body.

---

## What is deliberately not on this list

Sheet metal, surface and T-spline modelling, mesh repair, simulation and CAM are
whole product areas, not missing ops. Sketch constraints are not a gap either: a
constraint solver exists to make a drawing consistent, and a script that says
`gap / 2 + armT / 2` is consistent by construction — the trade this DSL makes on
purpose, and `v-block.js`'s exact 90° vee is what it buys.

## Suggested order

Done, in this order: the hole standards table (which found the export-collision
hazard on the way through), draft, torus, `pipe()`, the `UnifySameDomain`
segfault fix by vendoring OCCT 8.0.1, then loft and sweep of an authored profile
— the hold lifted as the product decision recorded in §4.

1. **Arcs in a section** (§2) — the general version of what the torus does for
   one shape: grooves, seats, radiused shoulders. `examples/hydraulic-line.js`'s
   groove is round-bottomed because a torus is all there is; a gland section is
   rectangular and wider than the cord, and this is what would let one be drawn.
2. Everything else: hold, with the reason recorded above rather than the
   intention.

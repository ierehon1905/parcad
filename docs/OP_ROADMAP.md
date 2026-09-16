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
| Extrude a sketch profile | ✅ `extrude`, `ngon` | lines, arcs, rounded corners and splines, re-entrant allowed; §2 |
| Revolve a sketch profile | ✅ `revolve`, `cone`, `countersink` | the same section type; §2 |
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
| Sweep | ✅ `sweep(profile, path, { bend, taper })`, `pipe(path, dia, { bend, taper })` | runs and bend arcs, a `{ helix }` or a `{ spline }` |
| Loft | ✅ `loft(sections, { smooth })` | §4 |
| Section view | ✅ viewport plane, `section` on `evaluate_part` | §8 |
| Coil | ✅ `pipe({ helix }, dia)`, `sweep(profile, { helix })` | §5 |
| Thread | ✅ `threadedRod` `threadedHole` | ISO 68-1 basic 60° profile, named sizes or `{ diameter, pitch }`, a clearance for printing; measured against its closed form — §5 |
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

## 2. Arcs and splines in a section — **DONE**

An arc in the section is what makes an O-ring groove, a bearing seat, a radiused
shoulder, and a radius authored in section is exact where a rolling-ball fillet
on the solid is a surface fit. It shipped with splines, as one section type
(`crates/parcad-core/src/section.rs`, `SectionEntry` in the DSL):

- a section is a list of corners `[x, y]`, closed by itself; `{ at, round }`
  is a corner rounded between its two straight edges;
- between two corners, `{ through: [x, y] }` or `{ radius: r }` is a circular
  arc, `{ spline: points, start, end }` a chord-length cubic through points,
  `{ bezier: controls }` and `{ bspline: poles, degree }` curves by control
  points; `[{ spline: points }]` alone is a closed C2 curve;
- the same type is the profile of `extrude`, `revolve`, `loft` (whose first or
  last section may be `{ z, point }`) and `sweep`, and `pipe`/`sweep` take
  `{ spline: [[x, y, z], ...] }` as a path.

**Prior art, and why this spelling.** CadQuery (`lineTo`, `threePointArc`,
`radiusArc`, `spline` over `GeomAPI_Interpolate`, `close`) and build123d
(`Polyline`, `ThreePointArc`, `RadiusArc`, `Spline`, `Bezier`,
`FilletPolyline`, `make_face`) were read for the vocabulary, and SVG's path
commands for what a model already knows (`C` is the cubic Bézier; the `A`
command's large-arc and sweep flags were rejected as the classic way to get an
arc on the wrong side). A section is data rather than a builder chain because
the graph carries it verbatim and every export name is a reserved word: not
one new export was added. Three-point arcs (`GC_MakeArcOfCircle`) are the
unambiguous form; `radius` exists because drawings dimension arcs that way,
with the sign saying which side. Everything resolves **in the core**, not the
kernel: arcs become three points and a centre, every curve a clamped B-spline
with explicit poles, and the kernel builds exactly those poles
(`Edge::bspline`). That is why `spline` is its own documented interpolation
rather than `GeomAPI_Interpolate`, whose end-tangent estimate lives in C++ —
bounds, area, the axis check and a sweep's reach need the curve before the
kernel runs. A polygon takes the old construction bit for bit: all 94
surviving corpus cases re-recorded unchanged.

**Convexity lifted.** Extrude, revolve, loft and sweep sections may be
re-entrant; the reason recorded for refusing them was the implicit kernel's
missing distance field, and a prism or a revolution pairs nothing. A loft pairs
edges by index with `CheckCompatibility` off, so a re-entrant section pairs as
literally as a convex one. A polygon that crosses itself is refused by name in
the graph; a curve that crosses its outline by `BRepCheck`
(`SelfIntersectingWire`) on the section face. Draft still takes a convex
polygon, because its inset is a half-plane intersection.

**Measured**, exact B-rep read off each part's STEP: `stadium-extrude`
4000 + 500π = 5570.796327; `rounded-plate` 3961.371669; `domed-revolve`
8000π/3 = 8377.580410; `revolved-fillet-shoulder` Pappus 6169.962838;
`circle-section-ring` 2π²Rr² = 4934.802201; `parabola-bezier` Archimedes' 400;
`loft-to-a-point` 1000π; `swept-stadium-bend` Pappus 2611.421237;
`stepped-shaft` 1720π; `l-plate` 2700; `re-entrant-loft` 5250 (a frustum of an L); `spline-pipe-straight` 848.229790 against
848.230016 (the pipe shell's fit). Edges from arcs select as `curve: "circle"`,
from curves as `curve: "spline"` (`curve-edges-by-kind`). `hydraulic-line`'s
gland is a catalogue section now, its cut 283.712922 mm³ against Pappus to the
last printed digit.

**What it moved.** One Fusion target, `untitled2-v1`, both bodies from the
export's degree-5 pole rows (examples/fusion360/README.md). The other three it
was supposed to unblock were not waiting on curves once probed.

**Since added:** `{ fit: points, tolerance }`, a curve the kernel fits through
sampled points and *measures* — the worst distance from any point to the
built curve is `deviation_mm` in every report, and a fit that cannot hold its
tolerance or crosses itself is refused — and `inset(outline, d)`, the outline
stepped inward by the kernel's offset and measured before it is used. What
they close is geometry that arrives as points: a simulation, a scan, an
involute sampled from its equation. docs/DSL_GAPS.md has the measurements.

**And since:** `{ curve: (t) => [x, y], from, to, tolerance }`, a curve
given by a formula, drawn by the script — where the function lives — as C1
cubic Hermite pieces with a bound it states and, given the function's
derivative and a fourth-derivative bound, certifies; the kernel measures the
built curve against the function between the pieces. `spurGearOutline` draws
involute teeth on it, profile-shifted when asked, and `spurGearPair` meshes two
at the centre distance their shifts need. docs/DSL_GAPS.md has the
measurements.

**Still out:** a section with holes (cut a second solid), a periodic or rational B-spline entry, tangent
continuity asked for across a corner, and draft on a curved outline.

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

**Since added:** a spline path, `{ spline: [[x, y, z], ...] }`, the chord-length
cubic of §2, refused where it bends tighter than the section reaches (the
radius and where are in the message). **Still out:** a profile that twists
along the path.

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

`loft(sections, { smooth })` takes two or more outlines stacked along +Z, with
arcs and curves since §2, and a point as the first or last section. Sections
must resolve to the same edge count, because pairing is by index and taken
literally: a rotated outline authors a *twisted* wall on purpose,
which is what recreated UnTriangle v3, held to a closed form by
`eval/cases/twisted-loft.json`. `smooth: true` is Fusion's look, one surface
fitted through all sections, and a fit that bulges past the sections' own
bounding box by more than the slip tolerance is refused
(`eval/cases/refuse-bulging-loft.json`), so the graph's cheap bounds stay honest.
Sections may be re-entrant since §2: with the compatibility pass off the
pairing is the author's, re-entrant or not.

## 5. Coils — **DONE**; threads — **DONE**, after the measurement that held them was re-run

The hold was lifted by a model building a unicorn, which needed a spiral horn,
a tapering mane and a tail and faked them from stacked primitives. What shipped
is the honest version: `{ helix: { radius, pitch, turns, endRadius, hand } }`
as a path for `pipe` and `sweep`, and `taper` on both. The helix is a straight
line in a cylinder's or cone's parameter space; its fitted 3D curve is
*measured* against the exact helix (1e-8 mm on the corpus) and refused past
1e-4 mm. Five eval cases hold volume to closed forms at about 1e-6 of
BRepGProp, and the graph refuses a pitch that runs a turn into the next and a
section that crosses the axis at either end.

A **thread** was held back for a measurement: a helical groove cut through its
coaxial cylinder opened past two turns. Diagnosed, it was never the boolean —
parcad's seam-pcurve pass dropped a pcurve the groove's other strips of the
same cylinder still used, and a guard for a different part had fixed it on main
before the helix merged (docs/GOTCHAS.md). What shipped is `Op::Thread`, as
`threadedRod` and `threadedHole`: the ISO 68-1 basic profile, swept in the
axial plane along a helix of one edge per turn (FreeCAD's construction, chosen
over cq_warehouse's ruled faces, bd_warehouse's lofted loops and the MakeBottle
tutorial's `ThruSections` by measurement — GOTCHAS "Threads"), measured against
its closed form on every build. The **thread annotation on the node** — a
`PartReport` that says "M6 × 1, 12 deep" for a hole whose geometry is honestly
a 5 mm drill — is still worth having for machined parts, where the tap drill
remains the right drawing.

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

Then arcs and splines in a section (§2), which drew `hydraulic-line.js`'s
gland as the catalogue section it had to approximate with a torus.

Everything else: hold, with the reason recorded above rather than the
intention.

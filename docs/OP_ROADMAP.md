# Op coverage, measured against Fusion 360

What a mainstream parametric CAD tool offers, what parcad has, and what the
missing ones would cost *here*. Fusion 360 is the yardstick because it is the
tool most people who would use this have used; the comparison is about the
**operations**, not the interaction model.

The deciding rule is the project's, not Fusion's:

> **An op ships when both backends can be honest about it.** OCCT can build far
> more than the implicit field can describe exactly. Anything where the two would
> disagree about where the surface is gets refused, restricted to the case where
> they agree, or left out — never approximated in one of them.

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
| Move / Copy, Rotate, Scale | ✅ `.at()` `.rotate()` `.scale()` | non-uniform scale is refused in B-rep, deliberately |
| Mirror | ✅ `.mirror()` | |
| Rectangular / circular pattern | ✅ `grid` `polar` `around` `repeat` | DSL helpers, not graph ops — the graph shares one node |
| Fillet / Chamfer | ✅ `.fillet()` `.chamfer()` with selectors | constant radius / equal distance |
| Torus primitive | ✅ `torus(major, minor, { sweep })` | full ring or arc; the arc is what a bend is |
| Shell | ✅ `.shell()` | |
| Offset face / Thicken | ✅ `.offset()` | whole-body offset, not per face |
| Hole | ✅ `holeFor` `tapDrill` `clearance` `counterbore` | ISO metric coarse, M2–M20 |
| Draft | ✅ `extrude(..., { draft })` | §1 |
| Sweep | ✅ `sweep(profile, path, { bend })`, `pipe(path, dia, { bend })` | runs and bend arcs; authored profile is B-rep only, round profile exact in both; no spline path |
| Loft | ✅ `loft(sections, { smooth })` | B-rep only — the implicit backend refuses it by name; §4 |
| Section view | ✅ viewport plane, `section` on `evaluate_part` | §8 |
| **Coil / Thread** | ❌ | hold, and probably for good — §5 |
| **Rib / Web** | ❌ | sugar over what exists — §6 |
| **Split body / face** | ❌ | a different request from the section view; §8 |
| Sheet metal, Surface/T-spline, Mesh, Simulation, CAM | ❌ | out of scope by design |
| Sketch constraints, timeline, parameters | n/a | the DSL is the parametric model; JS is a better parameter table |
| Assemblies / joints | ❌ | one graph has one root — a real gap, not a near-term one |

---

## 1. Draft — **DONE**

Taper a face by an angle so a part releases from a mould or a sand core, as
`extrude(profile, height, { draft })` and `ngon(..., { draft })` rather than a
separate `Op`: a drafted extrusion is one solid with one set of faces, and
modelling it as "extrude then modify" would import Fusion's history model for no
gain. `revolve` does not take it — a turned section already carries its own
taper, and a drafted *revolve* means something else.

B-rep builds it as a loft between the outline and its inset copy, since
`BRepOffsetAPI_DraftAngle` is not bound and `Solid::loft` is; the inset is a
half-plane intersection, so it degrades by losing an edge rather than producing a
bow tie, and an angle the outline cannot carry is refused with the maximum
bisected for and named.

**What was learnt.** The undrafted field combines the wall term
and the end-cap term the way `cylinder` does, `hypot(max(a,0), max(b,0))`, which
is exact *because the walls meet the ends at a right angle*. Under draft they do
not, and that combination then **over**-reads outside an obtuse rim — an
overestimate, the one error an implicit field must never make, because the octree
prunes on it. A drafted extrusion takes the plain `max` instead, which
underestimates there like every other corner. `drafted-boss` in `eval/cases/`
checks both backends against the closed form for a square frustum (29282.008
mm³); `refuse-impossible-draft` checks the refusal.

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

A **torus primitive** was the cheap down payment and shipped: a circle revolved
about a parallel axis, exact in both backends (`hypot(hypot(x, y) - R, z) - r`,
with none of the clamping that made `revolve`'s exact form unusable), with a
`sweep` angle so an arc of it is a pipe bend. `eval/cases/torus.json` checks
volume *and* area against the closed form. Its main use was blocked and no longer
is: an O-ring groove — `cylinder(12, 14).cut(torus(11.5, 1))` — used to segfault,
not in the boolean but in the `UnifySameDomain` pass every result goes through,
on the coaxial circular seams the cut leaves. Vendoring OCCT 8.0.1 fixed it;
`eval/cases/torus-gland.json` no longer carries a `known_defect`.

**What the torus still cannot be is a catalogue gland.** That section is
rectangular and wider than the cord; a torus cut is a circle. The round bottom in
`examples/hydraulic-line.js` is the honest shape, and the rest waits on the
profile type — which is the argument for it, made by a part rather than a table.

## 3. Sweep — **DONE**, in two honesty classes

This entry originally filed the whole feature under "hold", which was too broad:
"an arbitrary spline path has no exact field" is not "sweep has no exact field".
A *path* is made of elements, and two of them are exact — a straight run is a
cylinder, a circular bend is a partial torus. That is what a tube bender does and
what CAM posts; the spline is the special case.

`pipe(points, diameter, { bend })` is exact in both backends. Without a bend
radius the corners are square and filled with a ball — inside the swept envelope,
fine for clearance, not a shape anybody can make; with one, the runs are trimmed
back to their tangent points and a `torus` arc joins them, and a bend that will
not fit refuses naming the largest radius the corner takes. `bent-tube` checks a
90° bend against the closed form (π·25·70 + π·15·25·π/2 = 7348.34 mm³), and
`pipe-run` checks the square-cornered form against a closed form that is *not*
the obvious one — two perpendicular runs overlap in a quarter of a Steinmetz
solid, and the ball that fills the corner is three quarters redundant.

`sweep(profile, path, { bend })` — `Op::Sweep`, via a new
`BRepOffsetAPI_MakePipe` binding — takes a convex outline along the same
run-and-bend path model, in the second honesty class §4 defines: B-rep only,
refused by name in the implicit backend, because an authored section along a bent
path has no exact field the way a circle's does. `swept-channel` holds it against
Pappus's closed form (9141.59 mm³, read 0.005% under by tessellation, the same
effect `bent-tube` records). The corner math is `pipe()`'s, moved into
`Op::sweep_spine` so both backends and the graph refuse the same paths with the
same numbers. A *round* profile should stay a `pipe()`.

**Still out:** a spline path. The distance to a cubic is a quintic, and even
solved it is the sort of expression that stopped `revolve`'s exact form from
pruning under intervals; now that the B-rep side no longer requires an exact
field, what keeps it out is the section-and-path authoring gap that blocks the
Fusion targets — there is no spline type in the graph to sweep along. A profile
that twists or scales along the path is out with it.

**The one that bit.** The pieces have to be unioned *in path order*. Fusing all
the arcs first builds a compound of solids that do not touch, and the boolean
that finally bridges them hangs on a two-bend route and segfaults on a three-bend
one. docs/GOTCHAS.md has it.

## 4. Loft — **DONE**, and the hold lifted deliberately

This entry said *hold*, because a loft between two arbitrary outlines has no
closed-form distance and the two backends would describe different solids. The
hold was lifted as a product decision — loft and sweep each appear in more of the
owner's real documents than revolve, which we did build (DSL_GAPS, "What
twenty-one real Fusion 360 designs actually needed") — and the way it was lifted
matters more than the op:

**The implicit backend refuses a loft by name; it does not approximate one.** An
approximate distance field would be the worst outcome available: probes, wall
thickness, renders and sections all run on the field, so a field that is quietly
wrong means an agent confidently measuring a part that does not exist. `Op::Loft`
lowers only in `backend.rs`; `sdf.rs` answers "no exact distance field — evaluate
this part with the B-rep backend", and `loft-frustum` asserts that refusal's
wording alongside the B-rep's closed-form agreement (a 40→20 mm square
prismatoid, 28000 mm³ exactly).

`loft(sections, { smooth })` takes two or more convex polygon outlines stacked
along +Z. Default walls are ruled, contained in the sections' hull by
construction, so the graph's cheap bounds are honest. Vertex pairing is by
outline index, taken literally, since OCCT's compatibility pass used to re-origin
the wires and silently untwist a loft (docs/GOTCHAS.md); sections must share a
point count, and a rotated outline authors a *twisted* wall on purpose — which is
what recreated UnTriangle v3, held to a closed form by
`eval/cases/twisted-loft.json`. `smooth: true` is Fusion's look, one surface
fitted through all sections; a fit can bulge past the sections' box, so
`backend.rs` *measures* containment on the built solid and refuses a bulge over
the slip tolerance. Sections stay convex for the extrude/revolve reason plus
loft's own: on a re-entrant outline the kernel's vertex pairing is a silent
guess.

**The measured cost, which the owner should weigh.** A part containing a loft or
an authored-profile sweep loses every field-backed capability at once:
`probe_part`, `measure_wall_thickness`, raymarched renders and the section view —
the whole of what an agent can do without looking. The freeform parts these ops
exist for are exactly the parts an agent can no longer inspect. The plausible way
back is a signed distance derived from the B-rep tessellation the worker already
produces: the mesher reports the deflection it achieved, so a mesh-derived field
carries a *known, reported* error bound rather than a silent one. Future work,
deliberately not smuggled in here.

## 5. Threads and coils — hold, deliberately

The helical sweep is out for the reason in §3, doubly so; and a "thread" faked
from a stack of tori is the silent approximation this project refuses, in the
failure mode most likely to reach a part that gets made. Every threaded hole in
`examples/` is drawn as its tap drill and says so, which is what a machine shop
drawing does.

The useful improvement is not geometry: a **thread annotation on the node**, so
`PartReport` can say "M6 × 1, 12 deep" for a hole whose geometry is honestly a
5 mm drill. A reporting change, and worth more than fake helices.

## 6. Rib / Web — sugar, not an op

Fusion's Rib grows a thin wall from a sketch line down to the body. Here that is
`union(body, box(...).at(...))`, and the gussets in `motor-mount.js` and
`pillow-block.js` already do it. A `rib(from, to, thickness)` helper in `dsl.ts`
would remove the arithmetic, the way `polar()` did. No graph change.

## 7. Hole feature — **DONE**

Fusion's Hole dialogue knows drill standards: pick M6 and it knows the tap drill
is 5.0, the close-fit clearance is 6.4, the countersink 12.0 × 90°. Every example
used to write those as literals with a comment, and the comments had drifted —
`shaft-coupler.js` and `timing-pulley.js` both called a *tap* drill a "clearance
drill" (correct hole, wrong word), while `motor-mount.js` uses the real
clearance, 5.5, for the same M5. Nothing checked either. Now `METRIC_FASTENERS`
(M2–M20), with `tapDrill`, `clearance(thread, fit)`, `counterbore` and
`countersink("M5")` reading from it, plus `holeFor(thread, depth, { fit, tapped,
through })` for the cutter: DSL_GAPS §1's "standards knowledge hides in
arithmetic" applied to the numbers a machinist knows by heart.

**The one surprise.** Every export becomes a parameter name in the script
sandbox, so shipping a function called `hole` broke four existing scripts that
had `const hole = cylinder(...)`. The cutter is called `holeFor` because of it;
the general hazard is DSL_GAPS §7, and it applies to every future export and to
parts already saved in a user's project folder.

## 8. Split and section — a view concern — done

Fusion's Section Analysis was the thing most missed while writing the corpus
(DSL_GAPS §0). Splitting a body for inspection is `intersect(part, box(...))`,
which works but produces a *different part*. The right answer was a clipping
plane, not an op: the section control on the viewport, and `section` on
`evaluate_part` for a model that has no window. The part is untouched — every
measurement in the reply is still of the whole solid. docs/PERCEPTION.md §7
records how it works and the two ways it can quietly cut nothing. Splitting a
body into two *parts* is still not here, and is a different request.

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
— B-rep only with a named implicit refusal, the hold lifted as the product
decision recorded in §4 with the capability cost that came with it.

1. **Arcs in a section** (§2) — the general version of what the torus does for
   one shape: grooves, seats, radiused shoulders. `examples/hydraulic-line.js`'s
   groove is round-bottomed because a torus is all there is; a gland section is
   rectangular and wider than the cord, and this is what would let one be drawn.
2. Everything else: hold, with the reason recorded above rather than the
   intention.

# Op coverage, measured against Fusion 360

What a mainstream parametric CAD tool offers, what parcad has, and what the
missing ones would cost *here*. Fusion 360 is the yardstick because it is the
tool most people who would use this have used; the comparison is about the
**operations**, not the interaction model — sketch constraints, timelines and
joints are a different product decision, noted at the end.

The deciding rule for everything below is the project's, not Fusion's:

> **An op ships when both backends can be honest about it.** OCCT can build far
> more than the implicit field can describe exactly. Anything where the two
> would disagree about where the surface is gets refused, restricted to the
> case where they agree, or left out — never approximated in one of them.

That rule is why this list is short rather than a wish list, and why some
entries below are marked *hold* with a reason instead of a plan.

---

## Where we stand

| Fusion 360 | parcad | note |
|---|---|---|
| Box / Sphere / Cylinder primitives | ✅ `box` `sphere` `cylinder` | centred on the origin, placed with `.at()` |
| Extrude a sketch profile | ✅ `extrude`, `ngon` | convex outlines; see *Draft* and *Arcs* below |
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
| **Draft** | ✅ `extrude(..., { draft })` | shipped — see below |
| **Sweep** | ~ `pipe(path, dia, { bend })` | runs and bend arcs, both exact; no spline path, no non-circular profile |
| **Loft** | ❌ | hold |
| **Coil / Thread** | ❌ | hold, and probably for good |
| **Rib / Web** | ❌ | sugar over what exists |
| **Split body / face, Section view** | ❌ | a view concern more than a geometry one |
| Sheet metal, Surface/T-spline, Mesh, Simulation, CAM | ❌ | out of scope by design |
| Sketch constraints, timeline, parameters | n/a | the DSL is the parametric model; JS is a better parameter table |
| Assemblies / joints | ❌ | one graph has one root — a real gap, not a near-term one |

---

## 1. Draft — **DONE**

**What it is.** Taper a face by an angle so a part releases from a mould or a
sand core. Fusion applies it to an existing face; the usual authoring is
"extrude with 2° draft".

**Why it matters here.** Every cast or moulded part in the corpus is drawn with
vertical walls, which is wrong for the process each of them names. It is the
most common single thing a manufacturable part has that these cannot express.

**What it takes.** Both backends already have the shape of the answer:

- Implicit: a drafted prism is the same construction as today's, with each
  half-plane tilted out of vertical. `signed = (p - a)·n` where `n` gains a z
  component of `sin(draft)` — still linear, still exact, still interval-safe.
- B-rep: `BRepOffsetAPI_DraftAngle` is not bound, but a drafted prism is a loft
  between the outline and its inset copy, and `Solid::loft` **is** bound. For a
  convex outline the inset is well defined (offset each edge inward by
  `height * tan(draft)`) up to the point where the polygon collapses, which is
  the case to refuse with a message naming the maximum angle.

**Shape of the API.** `extrude(profile, height, { draft })`, and `ngon(...,
{ draft })`, rather than a separate `Op` — a drafted extrusion is one solid with
one set of faces, and modelling it as "extrude then modify" would import
Fusion's history model for no gain. `revolve` does not take it yet: a turned
section already carries its own taper, and a drafted *revolve* means something
different from a drafted prism.

**What it cost, and what was learnt.** The estimate above was right about where
the work was — the polygon inset, done as a half-plane intersection so it
degrades by losing an edge instead of producing a bow tie — and wrong about one
thing that mattered. The undrafted field combines the wall term and the end-cap
term the way `cylinder` does, `hypot(max(a,0), max(b,0))`, which is exact
*because the walls meet the ends at a right angle*. Under draft they do not,
and that combination then **over**-reads outside an obtuse rim — an
overestimate, the one error an implicit field must never make, because the
octree prunes on it. A drafted extrusion takes the plain `max` instead, which
underestimates there, like every other corner. `drafted-boss` in `eval/cases/`
checks both backends against the closed form for a square frustum (29282.008
mm³) and `refuse-impossible-draft` checks that a draft the outline cannot carry
is refused by both, with the maximum angle bisected for and named.

## 2. Arcs in a section — the enabling change

**What it is.** Today a profile is a list of straight segments. An arc in the
section is what makes an O-ring groove, a bearing seat, a radiused shoulder, a
torus.

**Why it matters here.** It is the single missing piece behind several parts,
and it is *also* the thing that makes `Op::Fillet` unnecessary for turned work:
a radius authored in section is exact, where a rolling-ball fillet on the solid
is a surface fit.

**What it takes.** The profile type stops being `Vec<[f64; 2]>` and becomes a
list of segments and arcs. Convexity still decides: a convex arc bulging
outward has an exact field (distance to the centre, minus the radius), and the
half-plane max extends to it unchanged. `Edge::arc` is already bound, so the
B-rep side is a different `Edge` constructor in the same loop.

A **torus primitive** was the cheap down payment, and it shipped: a circle
revolved about a parallel axis, exact in both backends
(`hypot(hypot(x, y) - R, z) - r`, with none of the clamping that made
`revolve`'s exact form unusable), with a `sweep` angle so an arc of it is a
pipe bend. `eval/cases/torus.json` checks volume *and* area against the closed
form.

**Its main use was blocked, and no longer is.** An O-ring groove —
`cylinder(12, 14).cut(torus(11.5, 1))` — used to segfault, not in the boolean
but in the `UnifySameDomain` pass every result goes through, on the coaxial
circular seams the cut leaves. Vendoring OCCT 8.0.1 fixed it;
`eval/cases/torus-gland.json` no longer carries a `known_defect`, and
`examples/hydraulic-line.js` turns the groove into its inlet boss.

**What the torus still cannot be is a catalogue gland.** That section is
rectangular and wider than the cord; a torus cut is a circle. So the round
bottom is the honest shape, and the rest waits on the profile type below —
which is the argument for it, made by a part rather than by a table.

**Cost.** Medium for the full profile type; the torus itself is done.

## 3. Sweep — **DONE for the path elements that have an exact field**

**The correction first.** This entry originally said sweep was out, and filed
the whole feature under "hold". That was too broad, and the pushback was right:
"an arbitrary spline path has no exact field" is not the same statement as
"sweep has no exact field". A *path* is made of elements, and two of them are
exact — a straight run is a cylinder, a circular bend is a partial torus. That
is what a tube bender does, what a pipe routing standard specifies, and what
CAM posts; the spline is the special case, not the norm.

**What shipped.** `pipe(points, diameter, { bend })`. Without a bend radius the
corners are square and filled with a ball — inside the swept envelope, fine for
clearance, not a shape anybody can make. With one, the runs are trimmed back to
their tangent points and a `torus` arc joins them, which is the real part. A
bend that will not fit refuses and names the largest radius the corner takes.
`bent-tube` in `eval/cases/` checks a 90° bend against the closed form
(π·25·70 + π·15·25·π/2 = 7348.34 mm³), and `pipe-run` checks the square-cornered
form against a closed form that is *not* the obvious one — two perpendicular
runs overlap in a quarter of a Steinmetz solid, and the ball that fills the
corner is three quarters redundant.

**What is still out, and why.** A spline path: the distance to a cubic is a
quintic, and even solved it is the sort of expression that stopped `revolve`'s
exact form from pruning under intervals. A non-circular profile swept along a
path: for a *straight* run that is just an extrude, so the missing piece is
only the bend, which needs a general swept surface rather than a torus. Both
are honest gaps; neither is "sweep is impossible here".

**The one that bit.** The pieces have to be unioned *in path order*. Fusing all
the arcs first builds a compound of solids that do not touch, and the boolean
that finally bridges them hangs on a two-bend route and segfaults on a
three-bend one. docs/GOTCHAS.md has it.

## 4. Loft — hold

`Solid::loft` is bound and the B-rep side is nearly free. The implicit side is
not: a loft between two arbitrary outlines has no closed-form distance, and the
two backends would describe different solids.

Draft, above, is now built *on* that loft — the one case where the implicit
side has a closed form, because a drafted prism's walls are planes. That is the
argument for leaving general loft alone rather than the argument for adding it:
the mechanism is already earning its keep in the shape the field can describe.

## 5. Threads and coils — hold, deliberately

Fusion models a thread as either cosmetic or as a real swept helix. Here:

- The helical sweep is out for the reason in §3, doubly so.
- A "thread" faked from a stack of tori is the silent approximation this project
  refuses, and it is the failure mode most likely to reach a part that gets
  made.

Every threaded hole in `examples/` is drawn as its tap drill and says so, which
is what a machine shop drawing does. The useful improvement is not geometry: it
is a **thread annotation on the node**, so `PartReport` can say "M6 × 1,
12 deep" for a hole whose geometry is honestly a 5 mm drill. That is a
reporting change, not a kernel one, and it is worth more than fake helices.

## 6. Rib / Web — sugar, not an op

Fusion's Rib grows a thin wall from a sketch line down to the body. Here that is
`union(body, box(...).at(...))`, and the gussets in `motor-mount.js` and
`pillow-block.js` already do it. A `rib(from, to, thickness)` helper in `dsl.ts`
would remove the arithmetic, in the way `polar()` did. No graph change.

## 7. Hole feature — **DONE**

Fusion's Hole dialogue knows drill standards: pick M6 and it knows the tap drill
is 5.0, the close-fit clearance is 6.4, and the countersink is 12.0 × 90°.
Here every example writes those numbers as literals with a comment, and the
comments had already drifted: `shaft-coupler.js` and `timing-pulley.js` both
called a *tap* drill a "clearance drill" (correct hole, wrong word — fixed when
this page was written), while `motor-mount.js` uses the real clearance, 5.5, for
the same M5. Nothing checks either.

`METRIC_FASTENERS` (M2–M20), with `tapDrill`, `clearance(thread, fit)`,
`counterbore` and `countersink("M5")` reading from it, plus
`holeFor(thread, depth, { fit, tapped, through })` for the cutter itself. It is
the "standards knowledge hides in arithmetic" complaint from DSL_GAPS §1,
applied to the numbers a machinist knows by heart.

**The one surprise.** Every export becomes a parameter name in the script
sandbox, so shipping a function called `hole` broke four existing scripts that
had `const hole = cylinder(...)`. The cutter is called `holeFor` because of it,
and the general hazard is DSL_GAPS §7 — it applies to every future export, and
to parts already saved in a user's project folder.

## 8. Split and section — a view concern — done

Fusion's Section Analysis is the thing most missed while writing the corpus (see
DSL_GAPS §0). Splitting a body for inspection is `intersect(part, box(...))`
today, which works but produces a *different part*. The right answer was a
clipping plane in the viewport, not an op, and that is what it is: the section
select and slider in the title bar, and `section` on `evaluate_part` for a model
that has no title bar. The part is untouched — every measurement in the reply is
still of the whole solid. docs/PERCEPTION.md §7 records how it works and the two
ways it can quietly cut nothing.

Splitting a body into two *parts* is still not here, and is a different request
from this one.

---

## What is deliberately not on this list

Sheet metal, surface and T-spline modelling, mesh repair, simulation and CAM are
whole product areas, not missing ops. Sketch constraints are not a gap either:
a constraint solver exists to make a drawing consistent, and a script that says
`gap / 2 + armT / 2` is consistent by construction — that is the trade this DSL
makes on purpose, and `v-block.js`'s exact 90° vee is what it buys.

## Suggested order

1. ~~Hole standards table~~ — **done**, and it found the export-collision
   hazard on the way through.
2. ~~Draft~~ — **done**, measured against a closed form in both backends.
3. ~~Torus~~ — **done**, with a swept arc for bends.
4. ~~`pipe()`~~ — **done**, runs and bends, measured against closed forms.
5. ~~Fix the `UnifySameDomain` segfault~~ — **done**, by vendoring OCCT 8.0.1,
   and `examples/hydraulic-line.js` has its groove back.
6. **Arcs in a section** — the general version of what the torus does for one
   shape: grooves, seats, radiused shoulders. The groove in 5 is round-bottomed
   because a torus is all there is; a gland section is rectangular and wider
   than the cord, and this is what would let one be drawn.
7. Everything else: hold, with the reason recorded above rather than the
   intention.

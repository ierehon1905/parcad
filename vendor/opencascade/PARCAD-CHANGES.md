# Changes from upstream opencascade 0.2.0

Kept as small as possible so the diff stays readable and can go upstream. Every
addition but one is a thin wrapper over a call `opencascade-sys` already binds.
The exception is `check_validity`, which needed a new C++ shim and so a fork of
the sys crate too — see `vendor/opencascade-sys/PARCAD-CHANGES.md`.

## Why fork at all

`Shape` holds its `TopoDS_Shape` in a `pub(crate)` field and offers no way in or
out. Everything below could otherwise have lived in our own crate.

## Added

- `Shape::transformed(&gp_Trsf)` — the general escape hatch, via
  `BRepBuilderAPI_Transform`.
- `Shape::rotated(origin, axis, radians)` — right-handed, about an arbitrary
  line.
- `Shape::scaled_uniform(origin, factor)` — `gp_Trsf::SetScale` is uniform only.
  Non-uniform scaling needs `gp_GTrsf` and `BRepBuilderAPI_GTransform`, which
  the sys crate does not bind, so it is absent rather than approximated.
- `Shape::translated(by)` — unlike `set_global_translation`, which *replaces* a
  shape's location and so cannot be nested, this composes.
- `AdHocShape::make_sphere(center, radius)` — `BRepPrimAPI_MakeSphere` was bound
  and unwrapped.
- `Shape::single_solid()` — unwrap a compound holding exactly one solid.
  `BRepFilletAPI_MakeFillet` returns one of these, and the difference is
  invisible until a boolean against it quietly produces nothing.
- `Shape::internal_void_count()` — how many sealed internal voids the shape's
  solids enclose: shells beyond each solid's outer boundary, counted per solid
  so disjoint bodies are not mistaken for cavities. Two `TopExp_Explorer`
  walks, both already bound. Exists because a subtractive cut whose tool never
  breaks a face returns a watertight, plausible, unmanufacturable solid — a
  block with its tool's shape entombed — and the extra shell is the one fact
  that distinguishes it from a blind pocket without inventing a thickness
  threshold.
- `impl Clone for Shape` — several operations take `self` by value while the
  caller still needs the original. Cheap: `TopoDS_Shape` is a handle onto a
  refcounted `TShape`.
- `Shape::check_validity(exact)` — `BRepCheck_Analyzer`, asking OpenCASCADE
  whether a shape it built is actually valid. This is a different question from
  the `IsDone()` an operation reports about itself, and the two disagree: a
  `union { blend: 2 }` over a curved seam returns `IsDone() == true` and a solid
  OpenCASCADE simultaneously reports as carrying two `UnorientableShape` faces.
  See docs/GOTCHAS.md.
- Boolean operation history — `BooleanShape` now exposes the kernel's modified
  and deleted relations for exact input edges, alongside its created section
  edges. This is the primitive needed for ParcAD to compose stable feature
  provenance; it is not a source-level edge-index API.

- `Shape::topology_report()` and `Shape::write_brep(path)` — the diagnostic
  pair over the new wrapper bindings: a full face/wire/edge/vertex dump, and a
  native BREP export that preserves the exact topology STEP normalises away.
  Both exist for diagnosing kernel output the checks above have refused.

- `Shape::clean()` now sets the unifier's merge tolerances to 1e-4 mm and
  1e-4 rad instead of the defaults (1e-7 mm, 1e-12 rad), which only ever
  merge exactly coincident geometry. A fillet corner rebuilt through
  approximation places its vertices only to the vertex tolerance, so two
  collinear pieces of one line come back a few 1e-7 apart and stayed split.
  clean() also first drops the dead half of a stale seam representation
  (`Shape_drop_unused_seam_pcurves`), which is what lets the seam-side pair
  merge at all. Measured across the whole eval corpus: the only shape that
  changes is the tangent-blend retainer stock, which loses exactly the two
  spurious junctions the tangent-pinch fix leaves on the grazing
  generators.

- `Solid::loft_sections(wires, ruled)` — `loft` with the walls' shape made
  explicit. The bare `BRepOffsetAPI_ThruSections` ctor defaults to a smooth
  surface fitted through the sections; `ruled` walls run straight between
  consecutive pairs, which is the form whose extent the sections themselves
  bound. Needed a two-argument sys ctor, see the sys crate's changes.
  `CheckCompatibility` is off: the caller's vertex ordering is the pairing,
  taken literally. With it on, OCCT re-origins the wires to minimise twist,
  which silently rebuilt a loft between a square and its 90°-rotated copy as
  a straight prism — a twisted bar the sections spelled out, discarded
  without a word. The caller owes aligned orderings and equal vertex counts
  (parcad validates both before calling).
- `Shape::sweep_profile_along(profile, spine)` — `BRepOffsetAPI_MakePipe`,
  sweeping a profile face along a spine wire. New sys binding, same pattern as
  the other `construct_unique` ctors.
- `Shape::geometry_json()` — measured geometry of a shape as JSON, over the
  sys crate's new `Shape_geometry_json`. Exists so parcad can read a STEP
  export from another CAD system back into authorable numbers: solids with
  exact mass properties, faces with their surface data down to B-spline pole
  grids, wires as ordered edge loops.

- `Shape::faces_json()` — what each face of the first solid *is*, over the sys
  crate's new `Shape_faces_json`: kind, exact area, centroid, placement
  direction and radius, and the faces it shares an edge with. The compact
  companion to `geometry_json`, for describing a part rather than recreating
  one. It exists because the full report is written on every rebuild otherwise
  and costs three times the time and four to six times the bytes to serialise
  boundary wires and B-spline pole grids the caller then discards; the
  measurements are in the wrapper's own comment.

- `ParcadEdgeTreatment::build()` (include/history.hxx) catches
  `Standard_Failure` instead of letting the raise escape the bridge and
  terminate the process, and the new `failure()` accessor returns what was
  raised — exception type and message, the kernel's own words. OCCT signals an
  unbuildable fillet this way routinely (`StdFail_NotDone` from the result
  accessor, `Standard_Failure("There are no suitable edges…")` from ChFi3d),
  so an uncaught raise turned an ordinary refusal into a dead worker.
  `treat_edges_with_history` reads that outcome, which changes
  `fillet_edges_with_history` / `chamfer_edges_with_history` to
  `Result<Vec<Shape>, String>`; previously they ignored `build()`'s bool and
  aborted in `result()`.

- `Shape::filleted_edges` / `Shape::chamfered_edges` — the fallible,
  non-mutating forms: build the treatment against `&self` and return the new
  shape, or the reason OpenCASCADE raised. Non-mutating on purpose, so a
  caller's failure path can probe several radii against one input while
  composing a refusal that names a radius measured to work.

- `Mesh::faces` — a `FaceRun { face, start, count }` per face, in triangles,
  saying where each face's triangles landed in `indices`. The mesher already
  walks the shape face by face and concatenates the per-face triangulations; the
  boundary between them was simply dropped on the floor. Recording it costs one
  push per face and is the only thing that lets a triangle be traced back to the
  `TopoDS_Face` it came from — without it a viewer can say "you are pointing at
  the solid" and nothing more precise.

  `face` is the index in the shape's own face traversal, counted across the
  `continue` that skips a face with no triangulation. That distinction is the
  whole reason the field exists: the runs are otherwise in emission order, which
  is shorter than the face count on exactly those shapes, and a caller using a
  run's position would name every face after the gap as its neighbour — wrong,
  but still adding up to a plausible total.

- `Shape::write_stl(path, deflection)` takes the tessellation tolerance
  instead of hard-coding 0.001 mm. `BRepMesh_IncrementalMesh` re-meshes any
  face whose existing triangulation is coarser than asked, so the old value
  threw away the 0.01 mm mesh the caller had just built and rebuilt every
  face ten times finer: on a plate with eighteen filleted bosses that was
  1.1 s a boss, against 30 ms to mesh them, and the whole file was still only
  going to a slicer. Passing the mesher's own tolerance writes the shape as it
  stands. `Solid::write_stl` and the `adhoc` demo keep their literal.

- `Mesher::mesh()` negates the normals of a face whose orientation is not
  `Forward`. The mesher already flipped such a face's triangle winding, and
  `BRepLib_ToolTriangulatedShape::ComputeNormals` gives the surface's own
  normals, which for a reversed face point into the material. Measured on a
  plain box rendered from below through parcad's one-sided shading: the
  underside came out at 12/255 before and 133/255 after, the same as its top.
  The face's `TopLoc_Location` is still not applied to the normals (the
  vertices do get it); no shape parcad builds has been seen to carry one.

## Changed

- `include/history.hxx` no longer names `TopTools_ListOfShape` or
  `Standard_True`, both deprecated in OCCT 8.0 — it includes
  `NCollection_List.hxx` and spells the type `NCollection_List<TopoDS_Shape>`,
  which is what that alias' own header names as the replacement. Three
  `-Wdeprecated-declarations` and one `-W#pragma-messages` per build. The
  matching change on the other side of the bridge is in
  `vendor/opencascade-sys/PARCAD-CHANGES.md`.
- `ParcadBoolean` builds its boolean once, in parallel. It used the
  `BRepAlgoAPI_Fuse(S1, S2)` / `BRepAlgoAPI_Cut(S1, S2)` constructors, which
  call `Build()` themselves, and then called `Build()` again — every fuse and
  cut ran twice (`SetToFillHistory(true)` between them was already the
  default). It now default-constructs, `SetArguments` / `SetTools`, and sets
  `SetRunParallel(true)`: OCCT's global parallel mode is off, so the
  face/face and curve-on-surface loops ran on one core. Measured on a
  530-node sculpture of overlapping spheres, cones and pipes (M4 Max): build
  13.3 s → 3.2 s, with volume, area, topology and the exported STL
  byte-identical; the `parcad-eval` corpus is 110/110 unchanged.
- `BooleanShape::cut_all(base, tools)` / `fuse_all(base, tools)` — one
  boolean against several tools, through `parcad_boolean_with_history`,
  `ParcadBoolean::add_tool` and `ParcadBoolean::build`; the two-shape
  constructors are now that sequence with one tool. OCCT treats the tools as
  a group (the cut removes their union; they may overlap), so the result and
  its history no longer depend on the order the tools were applied in —
  parcad judged a cut's sealed voids after each tool, and refused a cavity
  that a later tool in the same cut opened. Not used for a common: a
  multi-tool `BRepAlgoAPI_Common` intersects with the tools' union, which is
  not an n-way intersection.
- `Shape::internal_void_bounds()` — the tight box of each cavity shell, the
  outer shell being the one with the largest box. `internal_void_count`'s
  walk, with `bounds_optimal` per shell; lets a refusal say where a void is.

## Not added

- `BRepOffsetAPI_MakeOffsetShape`, for a general outward offset. Missing from
  `opencascade-sys` too, so it needs a new cxx binding and a C++ shim — a larger
  job than this fork.


## `Shape::signed_volume`

`BRepGProp::VolumeProperties` was already bound and unused by the crate; this
exposes its mass with the sign OCCT gives it, which is negative for a solid
whose faces point inward. parcad's offset lowering uses it to catch the
inside-out result `MakeThickSolid` returns for a filleted body — the shape a
later boolean reads as everything except the part.

## `Shape::oriented_outward`

`BRepLib::OrientClosedSolid` through the sys crate's new binding, on the
single solid this shape is; a compound or a shell passes through. The pair
with `signed_volume`: measure, then fix.

## `Edge::is_reversed`

The edge's `TopAbs_Orientation`, through the already-bound shape accessor. An
edge explored from a face carries the composed orientation, so this is the
wire's direction of travel on that face, and with the face's outward normal it
gives the side the face lies on. parcad reads the dihedral angle of every
edge from it: convex, concave, or tangent-continuous.

## Face history: `BooleanShape::modified_face`, `is_deleted_face`, `Treatment`

The boolean history already answered what an *edge* became; `Modified` and
`IsDeleted` are shape-generic in OCCT, so the same question is now asked of
faces. `ParcadEdgeTreatment` gains the same two calls, and
`fillet_edges_with_history` / `chamfer_edges_with_history` return a
`Treatment` that keeps the builder alive: what each treated edge generated,
and what any input face or edge became. `Face: Clone` and `Edge::from_shape`
are the small pieces that let parcad carry named faces through a transform.
This is what lets a tag mean "these faces" after every later operation.

## `Shape::into_unified` and `Unification`

`clean()` — the same-domain unify pass after every boolean — now has a form
that keeps `ShapeUpgrade_UnifySameDomain::History()`: which face or edge of
the input a merged face or edge came from, through `BRepTools_History`. The
boolean's own history stops at the boolean, and the coplanar faces the unify
pass merges are new to it; this is the missing half that lets a name on a face
survive a union whose faces it shares.

## `Shape::classify_point`, `distance_to_point`, `ray_caster`, `bounds_optimal`

The perception primitives, over the sys crate's new bindings of the same
names: which side of the boundary a point is on (`BRepClass3d`), how far it is
from the boundary and where (`BRepExtrema_DistShapeShape` against a vertex),
every place a line meets the boundary with the direction it crosses the
material (`BRepIntCurveSurface_Inter`, loaded once as a `RayCaster` and fired
many times), and a tag's exact extent (`BRepBndLib::AddOptimal`). These are
what parcad's probes, wall-thickness sweep and tag extents run on now that the
exact kernel is the only one; they used to read a distance field.

## `Shape::nearest_boundary`

A `NearestBoundary` over the sys crate's class of the same name:
`nearest_within(point, within)` for the boundary point nearest a point, if one
is nearer than `within`, and the face it is on; `project(face, point)` for the
exact point and outward unit normal of one face near a point on it. What
parcad's inscribed-ball wall thickness runs on: each face's and edge's
projector is built once rather than once per question.

## `Shape::least_distance_to`

`BRepExtrema_DistShapeShape` through the sys crate's new binding: the least
distance between two shapes and the points it joins. The clearance half of
parcad's fit check.

## `parcad_tidy_faces`, after every unify

`include/history.hxx` rewrites two face representations BRepMesh cannot
triangulate, after `ShapeUpgrade_UnifySameDomain` in both `clean()` and
`into_unified()` (whose history now merges the rewrite's, so a name follows
the rebuilt face and a dropped edge reads as deleted). Geometry is untouched;
both are what a union of an operand with a rotated copy of itself leaves:

- INTERNAL / EXTERNAL edges in a face are dropped. A sphere unioned with
  itself turned about X keeps the copy's seam on the one result face as an
  internal wire, and BRepMesh meshed only the region that wire cuts off: a
  closed 727.70 mm³ fragment of an exact 4188.79 mm³ solid.
  `AllowInternalEdges(false)` does not remove them; it only stops the unifier
  making new ones.
- A face with no wires at all is rebuilt with its surface's natural bounds.
  The unifier welds two halves of a torus into exactly that, which BRepCheck
  accepts and BRepMesh skips.

Across the eval corpus the only shape that changed is `tangent-blend`, which
loses two internal imprint lines on its side walls (50 edges to 48; volume
unchanged). See docs/GOTCHAS.md, "A correct solid can mesh as a closed
fragment of itself".

## `Shape::scaled_axes`

A different scale factor on each axis, through the sys crate's new
`Shape_scaled_axes` (`BRepBuilderAPI_GTransform`). `None` when the builder
fails. What lets parcad's `scale(x, y, z)` build an ellipsoid in the exact
kernel instead of refusing it.

`Shape::signed_volume` integrates adaptively to a 1e-7 relative error rather
than with `BRepGProp`'s fixed-order default, which misreads B-spline faces —
the scaled shapes above among them.

## `sweep`: `Helix`, `Shape::sweep_shell`, `SweepFrame`

A second cxx bridge beside `history.rs` — `src/sweep.rs` and
`include/sweep.hxx`, compiled by `build.rs` — so the whole addition is two new
files and one line in each of `build.rs` and `lib.rs`. Nothing in
`opencascade-sys` changed; every OCCT class it uses is reached from the C++
side only, and the toolkits (`TKOffset`, `TKGeomAlgo`, `TKTopAlgo`) were
already linked.

- `Helix::spine` — a helix about +Z as a one-edge wire: a `Geom2d_Line` on a
  `Geom_CylindricalSurface` (or a `Geom_ConicalSurface`, when the end radius
  differs), made into an edge and given a 3D curve by
  `BRepLib::BuildCurve3d(edge, 1e-7, GeomAbs_C2, 14, 1000)`.
- `Helix::deviation` — the largest distance, over evenly spaced parameters,
  between that fitted 3D curve and the analytic helix at the same parameter.
  The curve is an approximation; this is how far, measured, so the caller can
  refuse instead of assuming.
- `HelixByTurn::spine` — a cylindrical helix of a whole number of turns as a
  wire of one edge per turn, each a `Geom2d_Line` segment on the same
  `Geom_CylindricalSurface` with its own `BuildCurve3d` fit: FreeCAD's
  `makeLongHelix` construction. Every ISO coarse thread swept along it reads
  its closed form within 1e-5, where the one-edge spine reads up to 3e-5.
  `HelixByTurn::deviation` measures it against the analytic helix turn by turn.
- `Shape::sweep_shell(profile, spine, frame, scale_end)` —
  `BRepOffsetAPI_MakePipeShell` with a corrected-Frenet, Frenet or fixed-+Z
  binormal trihedron, `Add` or (when `scale_end != 1`) `SetLaw` with a
  `Law_Linear` from 1 to `scale_end` over [0, 1], then `Build` and
  `MakeSolid`. `Standard_Failure` is caught at the C++ boundary and returned
  as the `Err` string rather than terminating the process.

The law's parameter is worth knowing: `BRepFill_Sweep` maps it across spine
edges by curvilinear length at the edge boundaries but linearly in each edge's
own curve parameter inside one, which is length on a line or an arc and turn
angle on the helix edge above.

## `curve`: `Edge::bspline`, `Shape::loft_through`, `LoftProfile`

A third bridge, `src/curve.rs` over `include/curve.hxx`, added for sections
made of arcs and splines.

- `Edge::bspline(poles, knots, mults, degree)` — a non-rational, non-periodic
  `Geom_BSplineCurve` from explicit poles, distinct knots and multiplicities,
  made into an edge. The upstream `Edge::spline` is an empty stub; this takes
  poles rather than through-points on purpose, because parcad resolves every
  spline, Bézier and B-spline section entry to its poles before the kernel
  runs (see `crates/parcad-core/src/section.rs`).
- `Shape::loft_through(sections, ruled)` — `BRepOffsetAPI_ThruSections` into a
  solid with `CheckCompatibility(false)`, like `Solid::loft_sections`, where a
  section is `LoftProfile::Wire` or `LoftProfile::Point` (`AddVertex`, first
  or last only). Returns a `Shape` and an `Err` string on a builder failure or
  a caught `Standard_Failure` instead of casting an unbuilt result.
- `Edge::fit(points, tolerance, closed)` — a cubic C2 B-spline least-squares
  fitted through the points on uniform knots, doubling the number of spans
  from 4 until the curve *measures* within `tolerance` of every point (each
  point's distance to the curve, not the fitter's parametric criterion), or
  refusing when that would take a pole per point — interpolation, which is
  the overshoot a fit exists to avoid. Driven through `AppDef_BSplineCompute`
  the way `GeomAPI_PointsToBSpline` drives it (chord-length parameters
  normalised to [0, 1], no parameter correction, plain least squares) but
  with what that class hides: `SetKnots`, and `SetConstraints` — an open fit
  passes through its first and last point exactly, so a wire closes on the
  corners to the bit; a closed fit is the loop back to its first point with
  one tangent imposed at both ends (`AppParCurves_TangencyPoint`, the chord
  through the point's two neighbours at the parameterisation's own speed),
  so the seam is C1 by construction. The `FitReport` beside the edge carries
  the deviation measured on the curve returned, the pole count, and the
  curve sampled eight times per span for the caller's own checks. Two
  measurements chose the knots: `GeomAPI_PointsToBSpline`'s adaptive knots
  fit a rough section in 111–147 poles each, and a fifteen-section smooth
  loft through them ran past ten minutes, because `ThruSections` unifies
  every section's knots and adaptive vectors unify into their union; uniform
  vectors doubled from 4 unify into the finest of them, and the same loft
  builds in 41 s. And the fitter's own tolerance is not read at all: driven
  with parameters in millimetres it fitted nothing but the first sliver of
  the range and fell back to interpolation while reporting done, and driven
  correctly it still reports done on the interpolation fallback.
- `Wire::inset(distance)` — a closed planar outline stepped inward with
  `BRepOffsetAPI_MakeOffset` on the face the wire bounds, `GeomAbs_Intersection`
  joins so a polygon keeps its edge count, `Perform(-distance)`. Measured, not
  trusted: an `InsetReport` carries the largest distance any of 65 samples
  per result edge is from lying exactly `distance` inside the outline (the
  outline's curves projected, ends included), both areas, and the pole count;
  the caller refuses on slip or on an area that did not shrink. Both areas
  are Green's theorem over dense samples of the wires, not
  `BRepGProp::SurfaceProperties`, which read a lamp section's face 4 % small
  and had the guard refusing a good inset for growing (docs/GOTCHAS.md, "The
  volume integral misreads a wavy B-spline wall"). `Err` when nothing is
  left or the result is several loops. Three things the builder needed doing
  for it: it returns nothing, at any distance, for an outline that is one
  closed edge, so such an outline is split at its middle parameter before
  the offset and the result joined back into one edge with
  `GeomConvert_CompCurveToBSplineCurve` (exact for the B-spline and rational
  arc pieces the offset makes), its last pole set onto its first so the edge
  is closed to the bit; it starts its wire wherever it likes, so the result's
  edges are rotated and oriented to begin at the corner nearest the
  outline's start and run its way — a one-edge result re-origined at the
  curve point nearest that start — because `ThruSections` with the
  compatibility pass off pairs sections by first vertex and direction; and
  `GeomAPI_ProjectPointOnCurve` on the joined curve (some 2500 poles: the
  offset builder's own 3D approximations of its pieces) handed back an
  extremum a quarter of the way round from the true nearest point, so every
  nearest-point question here — deviation, slip, re-origin — brackets the
  span by sampling first and projects only within it.
- `Edge::deviation_from(points)` — the furthest any of the points lies from
  the edge's 3D curve, with the same bracket-then-project nearest-point
  search `Edge::fit` measures with. Added so a curve parcad builds from a
  function's poles is measured against points of that function it was not
  built through.
- `Wire::to_shape()` — the same handle as a `Shape`, borrowed; `From<Wire>`
  consumes and a wire is not `Clone`.
- Correction to `Edge::fit` above: the closed seam is G1, not C1.
  `AppParCurves_TangencyPoint` imposes the tangent's direction and the least
  squares solves each end's magnitude freely, so the two ends meet at
  different speeds, and a magnitude near zero cusps (docs/GOTCHAS.md,
  "`Edge::fit`'s closed seam is only G1, and can loop").
- `skin::Skinner` (`include/skin.hxx`, its own bridge) — a solid sewn from
  B-spline surfaces the caller computed: `set_surface` builds a non-rational,
  non-periodic `Geom_BSplineSurface` from a pole grid and knot vectors for
  the outer or the inner skin; `add_band` makes the face over the whole `u`
  range between two `v` values (`BRepBuilderAPI_MakeFace` on a copy of the
  surface cut to that range by `Geom_BSplineSurface::CheckAndSegment`, so no
  two bands share a surface handle and `ShapeUpgrade_UnifySameDomain` does
  not weld them back into one face after a boolean; the face shares the seam
  edge of a `u`-closed surface); `add_disc` the planar
  face a skin's `v` iso-curve bounds, and `add_ring` the planar face between
  the outer skin's iso-curve and the inner's at one height, its hole turned
  by `ShapeFix_Face::FixOrientation`. An iso-curve's last pole is set onto its
  first so the edge closes to the bit. `build` sews every face
  (`BRepBuilderAPI_Sewing`), refuses free edges or more than one shell, makes
  the solid, turns it to face the way `set_outward` stated — the outward
  direction at one `(u, v)` of a skin, compared with the normal of the sewn
  band there as the solid presents it, and checked again after turning — and
  refuses one `BRepCheck_Analyzer` does not pass. It refuses to build when no
  outward direction was stated. (It used `BRepLib::OrientClosedSolid`, whose
  one-ray classification reversed a correct pleated shell.) `measure_wall` is the wall between
  the two skins: from a grid of inner points, the nearest outer point found
  within one knot span of the same parameters (a coarse grid, then Newton on
  the squared distance held to that window) and the distance taken along
  the outer normal there, so an open end reads the wall continued; it
  returns the least and greatest and where each is.
  `measure_wall_reaching` widens that window in `v` to at least a given
  reach, for an inner skin whose point `(u, v)` is the offset of the outer
  one at a nearby `v`; `measure_wall` is it with no extra reach.
- `Mesher::new` and `Shape::write_stl` mesh in parallel
  (`BRepMesh_IncrementalMesh_ctor_full`, the sys crate's full constructor, with
  `isInParallel` set); the defaults for the other arguments are unchanged.
  Faces are meshed independently after their edges, so the triangulation is
  the serial one (docs/GOTCHAS.md, "One large B-spline face meshes far slower").
- `Shape::face_grid(per_side)` — points on every face, over the sys crate's
  `Shape_face_grid`: what a surface-to-surface distance is sampled at.
- `NearestBoundary::evaluate`, `edge_wedges`, `close_pairs` and
  `settle_pair`, with `EdgeWedge` and `ClosePair`: surface points by
  parameter, the angle between the faces along every edge, and pairs of faces
  that come close without sharing an edge, for the wall-thickness search.
  See `opencascade-sys/PARCAD-CHANGES.md`.
- `Mesh::face_uvs` — each vertex's own surface parameters, as the
  triangulation holds them; `uvs` is the same normalised per face for
  texturing, which cannot be evaluated on the surface.

## Self-intersection, orientation, and a treatment that leaves its input alone

Added for docs/VALIDITY_CHECKS.md: `BRepCheck_Analyzer` passes solids whose
faces cross and solids that are inside out.

- `Shape::self_interference(fuzzy, located)` and
  `Shape::self_interference_since(before, fuzzy, located)`, returning
  `SelfInterference { pairs, aborted, meetings }` — `BOPAlgo_CheckerSI` over
  the whole shape, or over the faces not in `before` against the faces whose
  boxes meet theirs. Thin wrappers over the sys shims of the same names.
- `Shape::orientation_faults()`, `Shape::turned_outward()`,
  `Shape::closed_solid()`, `Shape::reversed()` — over the sys shims of the
  same names.
- `Treatment::input()` — the shape the builder actually treated; see below.
- `FitReport::curve_poles` and `curve_knots` (`curve.rs`,
  `ParcadFit::curve_poles` / `curve_knots` in `include/curve.hxx`): the fitted
  curve itself, poles and full knot vector, empty for a periodic curve, so the
  caller can search it for crossings exactly instead of sampling it.

**Changed:** `ParcadEdgeTreatment` (`include/history.hxx`) builds on a
`BRepBuilderAPI_Copy` of its input's topology (geometry and triangulations
shared), and maps every history question from the caller's shapes through the
copy; a shape the treatment left alone is reported `modified` into its copy,
which is what the result holds. Measured before: `BRepFilletAPI_MakeFillet`
widens tolerances on the vertices and edges it is given, in place, including
on an attempt that builds and is then refused — one probe below a failed
2 mm blend left a vertex of the input at 42 mm tolerance, inherited by every
later probe and by anything sharing that vertex.
`a_treatment_attempt_leaves_the_shape_it_was_given_as_it_was` in
`parcad-occt` fails without the copy.

# Changes from upstream opencascade-sys 0.2.0

LGPL-2.1, unchanged. Our own crates are MIT OR Apache-2.0; this one is not, and
modifications to it stay under its licence.

## Why fork at all

Upstream binds no part of `BRepCheck`, so there was no way to ask OpenCASCADE
whether a shape it had just built was valid. That question is not answerable any
other way: `IsDone()` is each operation's opinion of itself, and it has been
observed returning `true` for a solid the kernel's own checker rejects.

The crate could not simply be upgraded instead. `opencascade-sys` was last
published in August 2023 at 0.2.0 and pins `occt-sys = 0.2` (OCCT 7.7.1); no
later release of it exists, so there is no version to bump to.

## Added

- `BRepCheck_report(shape, exact) -> String` (`include/wrapper.hxx`, declared in
  `src/lib.rs`). `""` for a valid shape, otherwise one `<kind> <n>: <status>`
  line per fault, where `<n>` is the position in a `TopExp_Explorer` walk of
  that kind and `<status>` is the `BRepCheck_Status` enum name from
  `BRepCheck::Print`.

  Bound as one report-producing call rather than as the `BRepCheck_Analyzer`
  class because the bool from `IsValid()` is not actionable on its own — the
  useful output is which sub-shape is wrong and why — and reaching
  `BRepCheck_Result` from Rust would mean binding several more OCCT collection
  types to learn the same thing.

  `exact` maps to the analyzer's `theIsExact` argument, enabling per-point
  checking. `GeomControls` is always `true`; with it off only topology is
  checked, and a face carrying an unusable surface passes.

  `BRepCheck` lives in `TKTopAlgo`, which `build.rs` already links, so no new
  toolkit was needed.

- `#include <sstream>` in `wrapper.hxx`, for the report's `std::ostringstream`.

- `Shape_topology_report(shape)` and `BRepTools_write_brep(shape, path)` in
  `wrapper.hxx`, both diagnostics. The report walks every face → wire → edge →
  vertex with geometry types, 3D and UV endpoints and tolerances, listing each
  wire twice — raw contents, then as far as `BRepTools_WireExplorer` can
  traverse it. A wire whose raw list is longer than its traversal is the
  signature of a rebuilt boundary gone wrong; that difference is what located
  the tangent-pinch defect in the fillet corner code (see
  `vendor/occt-sys/PARCAD-CHANGES.md`). The BREP writer exists because STEP
  export normalises exact topology away, and the report alone cannot be
  re-interrogated.

- `SetLinearTolerance` / `SetAngularTolerance` bound on
  `ShapeUpgrade_UnifySameDomain`. The defaults (1e-7 mm, 1e-12 rad) merge
  only exactly coincident geometry; the caller decides what "the same" means
  for shapes that went through an approximated rebuild.

- `Shape_drop_unused_seam_pcurves(shape)` in `wrapper.hxx` — healing for a
  boolean leftover: an edge that was a cylinder's seam can come out bordering
  its face only on one side (the wire references it once) while still
  carrying both seam pcurves, and that dead half stops `UnifySameDomain`
  from merging the edge with a collinear neighbour. Deliberately narrow: it
  touches only line generators on cylindrical faces, because on doubly
  periodic surfaces a boolean legitimately leaves a full boundary circle
  with both representations even though the wire uses it once, and the
  mesher needs them — the first, broader version of this pass opened the
  torus-gland groove by 168 mesh edges. Representation data only; geometry
  is untouched.

  It also skips a seam edge that another face *on the same surface* borders.
  A pcurve is stored per surface, not per face, so when a fuse splits one
  cylinder's side into two faces (a cylinder unioned with itself turned about
  its axis), each piece uses the old seam once and both representations are
  live; dropping one left the other piece's boundary on the wrong side of the
  period — BRepCheck invalid, 523.60 mm³ of a 1570.80 mm³ cylinder, and an
  open mesh. `eval/cases/coincident-cylinder-union.json`.

- `BRepOffsetAPI_ThruSections_ruled_ctor(is_solid, ruled)` — the existing
  ctor pins OCCT's second argument at its default (a smooth surface fitted
  through the sections); the lofting op needs to choose ruled walls
  explicitly. Same `construct_unique` pattern, one more argument.

- `BRepOffsetAPI_MakePipe` — type, ctor `(spine, profile)`, `Shape`, `Build`,
  `IsDone` — sweeping a profile face along a spine wire, plus its
  `#include <BRepOffsetAPI_MakePipe.hxx>` in `wrapper.hxx`. The general sweep
  op is built on it.

- `Shape_geometry_json(shape)` in `wrapper.hxx` — measured geometry as JSON,
  for reading a foreign B-rep (a STEP export from another CAD system) back
  into numbers a part can be authored from. Per solid: exact mass properties
  via `BRepGProp` and an optimal `Bnd_Box`; per face: the surface geometry —
  plane origin and outward normal, cylinder/cone/sphere/torus axes and radii,
  and for a B-spline surface the full pole grid with knots and multiplicities
  — plus every boundary wire in `BRepTools_WireExplorer` order with edge
  orientation applied, so a loop of lines reads directly as an ordered
  polygon. One call for the whole shape because the caller sits across a
  process boundary. The schema is deserialised by typed structs in parcad's
  `protocol.rs`, so a drift here fails loudly there. New includes for the
  `Geom_*` surface and curve classes it downcasts to.

  Each face also carries its exact area and centroid (`BRepGProp`), and
  `adjacent`: the faces it shares an edge with, as indices into the solid's own
  face order, from one `TopExp::MapShapesAndAncestors` per solid rather than a
  walk per face. A pair meeting along several edges is named once, and a seam
  edge does not make a face its own neighbour. Adjacency is the half of a face
  description that carries the part's *shape* rather than its dimensions — a
  plane at z=44 is the top of a plate or the floor of a pocket depending only
  on what it borders.

- `Shape_faces_json(shape)` in `wrapper.hxx` — the same per-face measurements
  for the first solid, without the boundary wires and without descending into a
  B-spline's poles, plus a compact `write_surface_placement` that reports only
  kind, direction and radius.

  It exists because the full report travels on every rebuild otherwise, behind
  a 120 ms editor debounce. Measured, best of 25 in-process: a drilled plate of
  18 planes and cylinders costs 2.09 ms and 13043 bytes through
  `Shape_geometry_json` against 0.73 ms and 2954 through this one; a six-face
  twisted loft, 0.46 ms and 5450 bytes against 0.14 ms and 877. Two writers
  rather than one writer with a flag, because the cost is in the writing — a
  filter would still have built the pole grid before dropping it.

  The face indices it reports are `TopExp_Explorer` order over the solid, which
  is the order `Mesh::faces` numbers by, so a triangle picked in a viewport and
  a face described here are the same face. That agreement is checked from Rust
  (`a_meshed_faces_triangles_lie_on_the_surface_the_report_describes`) rather
  than assumed, because it is a property of OCCT rather than of this code.

## Changed

- The deprecated spellings OCCT 8.0 warns on are gone from `wrapper.hxx` and
  from the cxx bridge in `src/lib.rs`. Together they were 178
  `-Wdeprecated-declarations` and 6 `-W#pragma-messages` on every build of this
  crate, which is more output than a real warning survives.

  `Standard_Integer`, `Standard_Real`, `Standard_True` and `Standard_False`
  become `int`, `double`, `true` and `false`; `GCE2d_MakeSegment` becomes
  `GC_MakeSegment2d` and `BRepCheck_ListIteratorOfListOfStatus` becomes
  `NCollection_List<BRepCheck_Status>::Iterator`, each the replacement its own
  header names.

  The `TColgp_`/`TopTools_`/`BRepCheck_` collection aliases moved to
  `src/Deprecated` in OCCT 8.0 and their headers are deprecated too, so
  `wrapper.hxx` includes the NCollection templates instead and declares its own
  aliases for them — cxx can only name a type by a plain identifier, and
  `NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>,
  TopTools_ShapeMapHasher>` is not one. Those aliases are what the bridge now
  names, so the Rust-side types and the helpers over them are renamed with
  them: `TopTools_ListOfShape` to `ListOfShape`, `TopTools_IndexedMapOfShape`
  to `IndexedMapOfShape`, `TopTools_IndexedDataMapOfShapeListOfShape` to
  `IndexedDataMapOfShapeListOfShape`, `TColgp_Array1OfDir` to `Array1OfDir`
  and `TColgp_Array2OfPnt` to `Array2OfPnt`. No warning is suppressed by a
  flag; the deprecated API is simply not used.

## Changed in `build.rs`

- `-std=c++17` instead of `-std=c++11`. OCCT 8.0 headers use constexpr and
  mutable-state idioms a C++11 compile rejects outright.
- The STEP and STL toolkits are linked under their new names. OCCT 8.0
  consolidated data exchange behind the DE framework: `TKSTEP`, `TKSTEPAttr`,
  `TKSTEPBase` and `TKSTEP209` became `TKDESTEP`, `TKSTL` became `TKDESTL`, and
  both now sit on XCAF — so `TKDE`, `TKXCAF`, `TKVCAF`, `TKCAF`, `TKLCAF`,
  `TKCDF`, `TKV3d` and `TKService` are linked too.

## Removed: `BRepLib_orient_closed_solid`

It bound `BRepLib::OrientClosedSolid` on a `TopoDS_Solid`, for the inside-out
offset `BRepOffsetAPI_MakeThickSolid` returns of a filleted body. Its one-ray
classification reversed a correct pleated shell, and every caller now uses
`Shape_orientation_report` / `Shape_turned_outward` (below). `Shape_closed_solid`
still calls `OrientClosedSolid` itself, and its result is measured by the
orientation report before anything uses it.

## Added: `BRepExtrema_least_distance`

`BRepExtrema_DistShapeShape` between two shapes: the least distance and the
two points it is measured between, or a negative number when the search does
not converge. parcad's fit check reports it as the clearance between a part
and the object it is meant to hold.

The search is default-constructed and loaded: the `(S1, S2)` constructor
already runs `Perform`, and the first version called `Perform` again after it,
measuring every clearance twice. It asks for the minimum only
(`Extrema_ExtFlag_MIN`, the one value read back) and runs multi-threaded.
Measured on `thread-m8-20-turns`, two 20-turn M8 rods: 16.9 s to 2.6 s for
the case, with the same recorded clearance.

## Added: `Shape_scaled_axes`

`BRepBuilderAPI_GTransform` with a diagonal `gp_GTrsf`: a different scale on
each axis, about the origin, returning an empty shape when the builder fails.
`gp_Trsf` is a similarity and cannot stretch one axis; the general transform
converts every surface to its exact B-spline form, which is how a sphere
becomes an ellipsoid.

## Added: `BRepGProp_VolumeProperties_eps`

`BRepGProp::VolumeProperties` with its relative-error argument, returning the
error estimate. The fixed-order integration behind the plain form is exact on
analytic faces and not on B-splines: the elliptic cylinder `Shape_scaled_axes`
makes of `cylinder(5, 20)` read 3168.66 mm³ against an exact 3141.59.

## Added: ray casting, point classification and exact bounds

The three primitives parcad's perception runs on once the exact kernel is the
only one — probes, wall thickness and tag extents used to be read off a
distance field the B-rep does not have.

- `BRepIntCurveSurface_Inter_load(intersector, shape, tol)` and
  `BRepIntCurveSurface_Inter_init_line(intersector, line)`: the two halves of
  the already-bound `Init(shape, line, tol)`, split because that call reloads
  the shape's face list for every line and a thickness sweep fires thousands
  of lines at one shape. `_w` is the hit's parameter along the line, `_state`
  where it lies on its face, and `_transition` which way the line crosses the
  *material* — the intersector reports the crossing against the surface's own
  normal, so a reversed face's answer is flipped here rather than by every
  caller.
- `IndexedMapOfShape_find_index(map, shape)`: `FindIndex` on the already-bound
  map, so a hit's face can be named by its traversal number — the number the
  mesher's face runs and the face report both use — with a hash lookup rather
  than a geometric comparison per hit.
- `BRepClass3d_classify(shape, x, y, z, tol)`: `BRepClass3d_SolidClassifier`,
  as a small integer for inside, outside, on the boundary, or undecidable.
- `BRepClass3d_classify_points(shape, points, tol)`: the same for many points,
  the solid loaded into one classifier and `Perform`ed per point, so checking
  a union against every input's faces does not rebuild it each time.
- `Shape_bounds_optimal(shape, …)`: `BRepBndLib::AddOptimal` off the exact
  geometry, no triangulation and no tolerance gap, so a tag's extent is the
  surface's own reach and not the mesh's.

Two new includes for them: `BRepClass3d_SolidClassifier.hxx` and
`GeomAdaptor_Curve.hxx`. Both classes are in `TKTopAlgo` / `TKG3d`, which
`build.rs` already links.

## Added: nearest boundary point, projectors kept

`NearestBoundary`, a C++ class in `wrapper.hxx`, with `NearestBoundary_new`,
`_nearest` and `_project`. The inscribed-ball wall thickness asks "is any
boundary point closer than r to here" a few times for each of thousands of
surface points, and `BRepExtrema_DistShapeShape` answers each question by
rebuilding every face's `Extrema_ExtPS` sample grid, every edge's
`Extrema_ExtPC` and every bounding box. This builds them once per shape, the
way `BRepExtrema_ExtPF::Initialize` and `BRepExtrema_ExtPC::Initialize` do,
and keeps them:

- `_nearest(x, y, z, within)` visits the faces whose optimal box is nearer than
  the best found so far, nearest box first; for each it takes every local
  extremum on the surface (`Extrema_ExtFlag_MINMAX`, since the least over the
  untrimmed patch can lie outside the face while a nearer one inside does
  not) that `BRepClass_FaceClassifier` puts in or on the face, then the face's
  edges and their end points, each edge once per query. It returns the
  distance, the point and a face it lies on, or a negative distance when
  nothing is within `within`.
- `_project(face, x, y, z)` is the nearest point of one face's surface to a
  point near it, and the outward normal there from the surface's first
  derivatives, reversed for a reversed face — or false when the classifier
  puts that point outside the face.

Faces are numbered as `TopExp::MapShapes` numbers them, the order
`IndexedMapOfShape_find_index` already reports. New includes:
`BRepClass_FaceClassifier.hxx`, `Extrema_ExtPC.hxx`, `Extrema_ExtPS.hxx`,
`Precision.hxx`, all in toolkits `build.rs` already links.

- `BRepMesh_IncrementalMesh_ctor_full(shape, deflection, relative,
  angular_deflection, in_parallel)` — the same `construct_unique` as the
  two-argument constructor, reaching `BRepMesh_IncrementalMesh`'s remaining
  arguments so the mesher can run faces in parallel.

- `Shape_face_grid(shape, per_side)` — points on every face: a grid over
  each face's `BRepTools::UVBounds`, evaluated by `BRepAdaptor_Surface` and
  kept where `BRepClass_FaceClassifier` does not put them outside the face.
  All of it already included.

## Self-intersection, orientation and closed shells

Added for docs/VALIDITY_CHECKS.md, all in `include/wrapper.hxx`, declared in
`src/lib.rs`; `TKBO`, `TKTopAlgo` and `TKBRep` were already linked.

- `Shape_self_interference_report(shape, fuzzy, located)` —
  `BOPAlgo_CheckerSI` on the shape, as `BRepAlgoAPI_Check` runs it. `""` when
  nothing meets; else `"<pairs> <aborted>"`, then `"<kind> <kind> x y z"` for
  the first `located` pairs, the point from `BRepExtrema_DistShapeShape`, or
  `"face itself x y z"` from the face's own `IntTools_FaceFace` when a face
  meets itself.
- `Shape_self_interference_since(after, before, fuzzy, located)` — the same
  over the faces of `after` that are not faces of `before` and every face
  whose box meets one of theirs. A `BOPAlgo_CheckerSI` subclass keeps only the
  candidate pairs with a changed face, or a sub-shape of one, on a side (a
  `BOPDS_IteratorSI` subclass filters its lists after `Intersect`), and
  intersects only the changed faces with themselves. Runs parallel.
- `Shape_orientation_report(shape)` / `Shape_turned_outward(shape)` — per
  solid, per shell: a point outside the solid's box classified
  (`BRepClass3d_SolidClassifier`) against each shell on its own must be
  outside the outer shell (the one with the largest box) and inside every
  other. `BRepClass3d::OuterShell` is not used: it classifies, and names a
  cavity on a solid that is inside out. Turning rebuilds the solid with each
  wrong shell reversed, through `BRepTools_ReShape`.
- `Shape_closed_solid(shape)` — a single solid as it is, or a single closed
  shell made a solid and turned by `BRepLib::OrientClosedSolid`; empty
  otherwise. `BRepOffsetAPI_MakeThickSolid` returns the inward offset of a
  treated or combined solid as a bare shell.
- `Shape_reversed(shape)` — `TopoDS_Shape::Reversed`, to make an inside-out
  solid in a test.

## Changed: the nearest-boundary query reads the face's own triangulation

`NearestBoundary` asks each face's `Extrema_ExtPS` for every point, and for a
surface OCCT has no closed form for — a B-spline, a surface of revolution or
extrusion, an offset — `Extrema_GenExtPS::Perform` rebuilds a grid of surface
samples per point. The classifier behind it, `BRepClass_FaceClassifier`,
intersects a line with every pcurve of the face per point. On a screw-top
jar's thread those two were 3 ms a ball. So, when the shape has been meshed:

- Each face keeps its `Poly_Triangulation` (nodes with the location applied,
  raw UV nodes, triangles) in a bounding-volume tree, with a `slack`: twice
  the largest distance between a triangle's middle, evaluated on the surface
  at the triangle's own interpolated parameters, and the chord there, plus
  the face's and its edges' tolerances. A face whose triangles are farther
  than the best distance so far plus that slack is skipped whole, boundary
  included.
- For a non-analytic surface the extremum search is replaced by damped
  Newton (Levenberg–Marquardt, exact Hessian, `D2`) from the nearest point of
  every triangle within `mesh distance + 2 slack` — every triangle that can
  hold the nearest surface point — keeping seeds three triangle sizes apart.
  Planes, cylinders, cones, spheres and tori keep `Extrema_ExtPS`, which
  solves them in closed form. The search is held inside the face's own
  parameter box (`BRepTools::UVBounds`), except along a direction the face
  covers for a whole period.
- Inside/outside is `BRepTopAdaptor_FClass2d`, built once per face on first
  use: the wires as polygons in the face's parameters, with the exact
  classifier behind them for a point within tolerance — what OCCT's own
  booleans use for repeated questions on one face.

A shape that has not been meshed is answered as before. New include:
`BRepTopAdaptor_FClass2d.hxx`, `Poly_Triangulation.hxx`, both in toolkits
`build.rs` already links.

## Added: surface points, edge angles and close face pairs

On `NearestBoundary`, for the wall-thickness search:

- `_evaluate(face, u, v, inside)` — the surface point and outward normal at
  parameters, optionally refusing a point outside the face.
- `_edge_wedges(spacing, out)` — every edge with exactly two distinct faces,
  sampled at most `spacing` apart and at least eight times, via its pcurve on
  each face (edges that do not share parameters with their pcurves are left
  out): eleven numbers a sample — edge, faces, point, the angle the material
  encloses between the faces (0 a knife, 90 a box edge, 180 smooth, over 180
  concave), how many faces' inward direction the classifier corrected, and
  the bisector into the material. The inward direction is the outward normal
  crossed with the edge's tangent as oriented in the face, checked once per
  edge and face by stepping along it and classifying.
- `_close_pairs(reach, out)` — pairs of faces that share no edge and whose
  surfaces may come nearer than `reach`: a dual traversal of the two faces'
  triangle trees finds triangle pairs within `reach + slack`, keeping the
  nearest pair in each reach-sized cell within twice the slacks of the least
  distance found and skipping a cell once it has one (a wall of even
  thickness is all in the band); pairs near a vertex the faces share are not
  counted. From up to eight of those, damped Newton on both surfaces at once
  settles a double normal; the least that is inside both faces is reported,
  or, when none is, the nearest triangles' points on the surfaces with the
  flag clear.
- `_settle_pair(a, b, near_a, near_b, ...)` — the same Newton from given
  points, for walking along a line of equal distances.

`_face_distance` over `BRepExtrema_DistShapeShape`, which this replaced, spent
40–60 ms a pair in `Extrema_ExtCC`'s global optimisation between B-spline
edges.

## `Shape_bounds_bracket`, and `NearestBoundary`'s box for a meshed face

`Shape_bounds_bracket` returns two cheap boxes around a shape's tight one: an
enclosing box from `BRepBndLib::Add` without the triangulation (control
points and tolerances), and the box of its triangulation's nodes, which lie on
the shape. parcad's tag extents optimise only the faces that can widen what
the nodes already reach.

`NearestBoundary` now bounds a face it has a mesh for with that enclosing box
rather than `AddOptimal`: the box only orders and prunes faces, a meshed face
is pruned by its triangles anyway, and `AddOptimal` on 546 offset B-spline
faces was most of the cost of building one. A face with no mesh keeps the
tight box.

## Not changed

Everything else is upstream 0.2.0 verbatim. The OCCT it builds against is **not**
the 7.7.1 this crate originally resolved: `occt-sys` is vendored beside it at
8.0.1. See `vendor/occt-sys/PARCAD-CHANGES.md`.

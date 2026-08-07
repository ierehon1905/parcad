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

## Not added

- `BRepOffsetAPI_MakeOffsetShape`, for a general outward offset. Missing from
  `opencascade-sys` too, so it needs a new cxx binding and a C++ shim — a larger
  job than this fork.

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
  Measured across the whole eval corpus: the only shape that changes is the
  tangent-blend retainer stock, which loses exactly the spurious junction
  the tangent-pinch fix leaves on the grazing generator.

## Not added

- `BRepOffsetAPI_MakeOffsetShape`, for a general outward offset. Missing from
  `opencascade-sys` too, so it needs a new cxx binding and a C++ shim — a larger
  job than this fork.

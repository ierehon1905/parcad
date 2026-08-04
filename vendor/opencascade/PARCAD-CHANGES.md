# Changes from upstream opencascade 0.2.0

Kept as small as possible so the diff stays readable and can go upstream. Every
addition is a thin wrapper over a call `opencascade-sys` already binds — nothing
here needed a change to the C++ shim.

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
- Boolean operation history — `BooleanShape` now exposes the kernel's modified
  and deleted relations for exact input edges, alongside its created section
  edges. This is the primitive needed for ParcAD to compose stable feature
  provenance; it is not a source-level edge-index API.

## Not added

- `BRepOffsetAPI_MakeOffsetShape`, for a general outward offset. Missing from
  `opencascade-sys` too, so it needs a new cxx binding and a C++ shim — a larger
  job than this fork.

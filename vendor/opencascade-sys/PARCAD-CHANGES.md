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

- `BRepOffsetAPI_ThruSections_ruled_ctor(is_solid, ruled)` — the existing
  ctor pins OCCT's second argument at its default (a smooth surface fitted
  through the sections); the lofting op needs to choose ruled walls
  explicitly. Same `construct_unique` pattern, one more argument.

- `BRepOffsetAPI_MakePipe` — type, ctor `(spine, profile)`, `Shape`, `Build`,
  `IsDone` — sweeping a profile face along a spine wire, plus its
  `#include <BRepOffsetAPI_MakePipe.hxx>` in `wrapper.hxx`. The general sweep
  op is built on it.

## Changed in `build.rs`

- `-std=c++17` instead of `-std=c++11`. OCCT 8.0 headers use constexpr and
  mutable-state idioms a C++11 compile rejects outright.
- The STEP and STL toolkits are linked under their new names. OCCT 8.0
  consolidated data exchange behind the DE framework: `TKSTEP`, `TKSTEPAttr`,
  `TKSTEPBase` and `TKSTEP209` became `TKDESTEP`, `TKSTL` became `TKDESTL`, and
  both now sit on XCAF — so `TKDE`, `TKXCAF`, `TKVCAF`, `TKCAF`, `TKLCAF`,
  `TKCDF`, `TKV3d` and `TKService` are linked too.

## Not changed

Everything else is upstream 0.2.0 verbatim. The OCCT it builds against is **not**
the 7.7.1 this crate originally resolved: `occt-sys` is vendored beside it at
8.0.1. See `vendor/occt-sys/PARCAD-CHANGES.md`.

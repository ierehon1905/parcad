// v4 v4 — Fusion 360 recreation target. DOES NOT BUILD YET.
//
// Exported from reference/fusion/v4/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   7375.19 mm^3
//   area     6349.63 mm^2
//   bbox     95.81 x 3.0 x 97.48 mm
//   faces    3 — nurbs 1, plane 2
//   solids   2
//
// Fusion built it with: ConstructionPlane, DeleteFace, Extrude, Fillet, Sketch, SplitBody.
//
// Blocked on a spline outline, before SplitBody ever comes up. The body is a
// 3 mm extrusion whose side wall is one NURBS surface: a closed *spline*
// sketch outline (six B-spline curves in the export) pushed through the
// thickness, then split. parcad's extrude takes a convex polygon outline, so
// the shape cannot be authored even before the split-and-keep-one-half step
// that parcad also lacks. See docs/DSL_GAPS.md and docs/OP_ROADMAP.md.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "v4 is a Fusion recreation target, not a part yet: " +
    "its outline is a spline, which extrude() cannot take, and parcad also has no " +
    "SplitBody. See examples/fusion360/README.md.",
);

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
// Blocked on SplitBody and DeleteFace: parcad has no way to cut a solid in two and keep one half. See docs/DSL_GAPS.md and docs/OP_ROADMAP.md for
// whether that op is coming and what it would cost.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "v4 is a Fusion recreation target, not a part yet: " +
    "parcad has no SplitBody and DeleteFace. See examples/fusion360/README.md.",
);

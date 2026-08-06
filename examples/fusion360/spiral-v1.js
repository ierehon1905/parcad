// Spiral v1 — Fusion 360 recreation target. DOES NOT BUILD YET.
//
// Exported from reference/fusion/Spiral-v1/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   406116.03 mm^3
//   area     31897.29 mm^2
//   bbox     85.14 x 95.7 x 100.0 mm
//   faces    3 — not recorded
//   solids   2
//
// Fusion built it with: CircularPattern, ConstructionAxis, ConstructionPlane, Extrude, Fillet, Loft, Sketch, Sweep.
//
// Blocked on Loft along a helix: no helical path, and no loft. See docs/DSL_GAPS.md and docs/OP_ROADMAP.md for
// whether that op is coming and what it would cost.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "spiral-v1 is a Fusion recreation target, not a part yet: " +
    "parcad has no Loft along a helix. See examples/fusion360/README.md.",
);

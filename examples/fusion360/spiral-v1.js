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
// Blocked on a helical loft — parcad now has loft, but this is not a stack of
// sections along +Z: the export's wall is one twisted surface carrying over a
// thousand B-spline curves, a loft whose sections rotate as they rise. It is
// also two solids, and the graph holds one. Neither half is reachable with
// the section types parcad has. See docs/DSL_GAPS.md and docs/OP_ROADMAP.md.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "spiral-v1 is a Fusion recreation target, not a part yet: " +
    "parcad's loft stacks sections along +Z and this spiral's sections rotate as they rise " +
    "(and it is two solids). See examples/fusion360/README.md.",
);

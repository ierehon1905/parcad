// Steam Top 4 Holed v1 v6 — Fusion 360 recreation target. DOES NOT BUILD YET.
//
// Exported from reference/fusion/Steam-Top-4-Holed-v1-v6/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   8975.79 mm^3
//   area     17313.39 mm^2
//   bbox     49.27 x 80.0 x 49.06 mm
//   faces    39 — cone 3, cylinder 14, nurbs 10, plane 6, torus 6
//   solids   1
//
// Fusion built it with: CircularPattern, ConstructionPlane, Extrude, Fillet, Patch, ReverseNormal, Revolve, Sketch, Stitch, Sweep, Thicken, Trim.
//
// Blocked on Patch: surface modelling: it closes a boundary with a patch, then thickens. See docs/DSL_GAPS.md and docs/OP_ROADMAP.md for
// whether that op is coming and what it would cost.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "steam-top-4-holed-v1-v6 is a Fusion recreation target, not a part yet: " +
    "parcad has no Patch. See examples/fusion360/README.md.",
);

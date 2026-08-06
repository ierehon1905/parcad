// шар v3 — Fusion 360 recreation target. DOES NOT BUILD YET.
//
// Exported from reference/fusion/v3/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   515661.06 mm^3
//   area     36026.52 mm^2
//   bbox     99.9 x 99.84 x 99.9 mm
//   faces    460 — cone 224, nurbs 32, plane 168, sphere 28, torus 8
//   solids   1
//
// Fusion built it with: CircularPattern, ConstructionPlane, Fillet, Sketch, Sphere, Sweep.
//
// Blocked on sweeping along curved rails — parcad now has sweep, but its
// path model is a bender's: straight runs joined by circular bends, profile
// held rigid. This ornament sweeps around the surface of a sphere (448
// circles and 18 ellipses in the export, 224 conical faces from the pattern,
// and 32 NURBS faces where the sweep and its fillets leave the analytic
// world entirely). See docs/DSL_GAPS.md and docs/OP_ROADMAP.md.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "v3 is a Fusion recreation target, not a part yet: " +
    "parcad's sweep follows straight runs and circular bends, and this part sweeps " +
    "around a sphere into NURBS. See examples/fusion360/README.md.",
);

// Untitled2 v1 — Fusion 360 recreation target. DOES NOT BUILD YET.
//
// Exported from reference/fusion/Untitled2-v1/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   36356.9 mm^3
//   area     7025.51 mm^2
//   bbox     36.85 x 36.85 x 81.01 mm
//   faces    1 — nurbs 1
//   solids   2
//
// Fusion built it with: Fillet, Revolve, Sketch.
//
// Blocked on a spline in section: revolve() takes a convex polygon profile,
// and this body is one NURBS surface — a spline revolved about the axis. A
// polyline stand-in for the spline would build and measure and be exactly
// the approximation this file refuses; the honest unblocking step is a
// richer section type (arcs first, then splines). It is also two solids in
// the document, of which the header records the first. See docs/DSL_GAPS.md
// and docs/OP_ROADMAP.md.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "untitled2-v1 is a Fusion recreation target, not a part yet: " +
    "revolve() takes a polygon section and this profile is a spline. " +
    "See examples/fusion360/README.md.",
);

// ваза v2 — Fusion 360 recreation target. DOES NOT BUILD YET.
//
// Exported from reference/fusion/v2/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   144241.62 mm^3
//   area     17337.7 mm^2
//   bbox     57.32 x 120.0 x 57.32 mm
//   faces    9 — nurbs 8, plane 1
//   solids   1
//
// Fusion built it with: ConstructionPlane, Loft, Sketch.
//
// Blocked on spline sections, not on Loft itself — parcad now has loft. Its
// sections are convex polygons; this vase is lofted through *spline* sketch
// outlines (16 B-spline curves in the export), and eight of its nine faces
// are the NURBS walls fitted through them. A polygon stand-in for those
// sections would build, measure, and be the silent approximation this file
// refuses. What it waits on is a spline (or at least arc) section type — see
// "arcs in a section" in docs/OP_ROADMAP.md, and docs/DSL_GAPS.md.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "v2 is a Fusion recreation target, not a part yet: " +
    "parcad's loft takes polygon sections and this vase is lofted through spline outlines. " +
    "See examples/fusion360/README.md.",
);

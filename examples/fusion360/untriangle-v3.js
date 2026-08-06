// UnTriangle v3 — Fusion 360 recreation target. DOES NOT BUILD YET.
//
// Exported from reference/fusion/UnTriangle-v3/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   31976.42 mm^3
//   area     12837.13 mm^2
//   bbox     14.14 x 117.68 x 106.66 mm
//   faces    30 — cylinder 3, nurbs 15, plane 12
//   solids   2
//
// Fusion built it with: CircularPattern, Combine, ConstructionPlane, Draft, Extrude, Fillet, Loft, Mirror, Sketch.
//
// The closest to reachable of the loft targets, and still not there. parcad
// now has loft, and this part's fifteen NURBS walls are bounded entirely by
// straight lines — a fitted loft through polygon sections, which is exactly
// the shape of parcad's `loft(..., { smooth: true })`. Two things stand
// between that and a recreation: the section outlines exist only inside the
// Fusion document (the export carries the fitted result, so they must be
// reverse-measured from it), and two kernels fitting a smooth surface
// through the same sections do not owe each other the same surface — whether
// OCCT's fit agrees with Fusion's to recreation standard is measurable now
// but has not been measured. It is also two solids. See
// docs/DSL_GAPS.md and docs/OP_ROADMAP.md.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "untriangle-v3 is a Fusion recreation target, not a part yet: " +
    "its loft sections live only in the Fusion document, and whether parcad's fitted " +
    "loft matches Fusion's surface is unmeasured. See examples/fusion360/README.md.",
);

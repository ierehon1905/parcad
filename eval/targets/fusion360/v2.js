// ваза v2 — Fusion 360 recreation target. BUILDS, BUT DOES NOT AGREE.
//
// Exported from reference/fusion/v2/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   144241.62 mm^3   (the export's B-rep: 144253.54)
//   area     17337.7 mm^2     (the export's B-rep: 17336.65)
//   bbox     57.32 x 120.0 x 57.32 mm
//   faces    9 — nurbs 8, plane 1
//   solids   1
//
// Fusion built it with: ConstructionPlane, Loft, Sketch.
//
// What the probe says it is. The "spline sections" this file used to be
// blocked on are not splines: evaluating the export's eight wall surfaces at
// their double knots gives the four sketches exactly — a 40 x 40 square at
// y = 0, a circle of radius 20 at y = 40 (to 1.3e-3 mm, the fit's own
// tolerance), the same square turned 45° at y = 80, and a point at y = 120.
// Arcs in a section and a loft that ends on a point both exist now, so the
// sections are authorable, and this builds:
//
//   const s = 20 * Math.SQRT1_2, r2 = 20 * Math.SQRT2;
//   const at = (deg) => [20 * Math.cos(deg * Math.PI / 180), 20 * Math.sin(deg * Math.PI / 180)];
//   const square = [[20, 0], [20, 20], [0, 20], [-20, 20], [-20, 0], [-20, -20], [0, -20], [20, -20]];
//   const circle = [0, 45, 90, 135, 180, 225, 270, 315].flatMap((d) => [at(d), { through: at(d + 22.5) }]);
//   const diamond = [[r2, 0], [s, s], [0, r2], [-s, s], [-r2, 0], [-s, -s], [0, -r2], [s, -s]];
//   return loft([
//     { z: 0, outline: square }, { z: 40, outline: circle },
//     { z: 80, outline: diamond }, { z: 120, point: [0, 0] },
//   ], { smooth: true }).rotate("x", -90);
//
// It measures (exact B-rep, its STEP probed): 144343.73 mm^3 (+0.07% on
// Fusion), 17678.97 mm^2 (+2.0%), 56.57 x 120 x 56.57 mm, 9 faces — nurbs 8,
// plane 1. Volume and face structure agree; area and width do not. Fusion's
// surface is a C1 bicubic, double knots at each section, that bulges 0.38 mm
// past the sections' own extent (28.66 against 28.28); OCCT's ThruSections
// fit is C2 and stays inside it, and parcad refuses a smooth loft that bulges
// past its sections by more than 0.05 mm anyway. Same sections, different
// fitted surface between them — the "two kernels' fits owe each other
// nothing" case in examples/fusion360/README.md — so this is not a faithful
// recreation, and it is not moved.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md.

throw new Error(
  "v2 is a Fusion recreation target, not a part yet: its sections now build as a smooth loft, " +
    "but OCCT's fitted wall is 2% short of Fusion's area and 0.75 mm narrower. " +
    "See eval/targets/fusion360/README.md.",
);

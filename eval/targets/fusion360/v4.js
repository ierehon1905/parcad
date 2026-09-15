// v4 — Fusion 360 recreation target. DOES NOT BUILD YET.
//
// Exported from reference/fusion/v4/. Measured in Fusion, so these are the
// numbers a recreation has to hit:
//
//   volume   7375.19 mm^3   (the export's B-rep: 6848.95)
//   area     6349.63 mm^2   (the export's B-rep: 5607.00)
//   bbox     95.81 x 3.0 x 97.48 mm
//   faces    3 — nurbs 1, plane 2
//   solids   2
//
// Fusion built it with: ConstructionPlane, DeleteFace, Extrude, Fillet, Sketch, SplitBody.
//
// Not blocked on a spline outline any more, and not only on SplitBody. What
// the probe of the export found (2026-09-15):
//
// - The header and the file disagree by 7% in volume and 12% in area; the
//   export is the specification, and its Body1 is not a prism. Its top face
//   (y = 5) is 931.55 mm^2 and its bottom (y = 2) 2483.18: the one NURBS wall
//   rolls over into the top face with a radius of about 3 mm, and Body2 (y 0
//   to 2) rolls under the same way. The wall is one bicubic surface across
//   both bodies, split at u = 0.96 — the fillets were merged into it
//   (DeleteFace) before the split.
// - Its outline at the split is not six authored sketch splines but a single
//   closed B-spline of about 390 poles with non-uniform knots: Fusion's
//   approximation after the fillet, which no section entry should copy.
//
// Section splines now exist (`{ spline }`, `{ bspline }`, and a closed
// `[{ spline: points }]` blob), and SplitBody on a slab is an intersection
// with a box. What is still missing is the sketch itself: the six splines
// Fusion's outline was drawn with are not in the export, and the fillet
// that rounds a spline-walled slab over 3 mm is Fusion's approximation. A
// recreation would be a fit to a fit, measured against a header that does not
// match its own file.
//
// This file throws rather than approximating. A stub that returned a rough
// solid would measure as a part and read as progress, which is worse than
// nothing — see "refuse rather than approximate" in CLAUDE.md. When it does
// build, check it against the numbers above and move it up into examples/.

throw new Error(
  "v4 is a Fusion recreation target, not a part yet: the export's outline is a ~390-pole " +
    "fitted curve with its fillets merged into the wall, not the authored sketch. " +
    "See eval/targets/fusion360/README.md.",
);

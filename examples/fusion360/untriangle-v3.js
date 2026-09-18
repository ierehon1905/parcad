// UnTriangle v3 — an impossible triangle, recreated from the Fusion 360 export.
//
// Three bars of 10 mm square section whose axes draw an equilateral triangle,
// each twisting a quarter turn between its corners so the flat of one end
// arrives as the edge of the next. Held to the export's own B-rep (Body12),
// which parcad reads with `parcad --probe-step`:
//
//   volume  24800.00 mm^3 against 24799.94   (+0.00024%)
//   area    11619.41 mm^2 against 11619.40   (+0.00005%)
//   bounds  10.00 x 132.32 x 114.59 mm       (exact)
//   faces   30, and the same 30: 18 plane, 12 nurbs
//
// The twelve twisted walls are Fusion's surfaces, not merely close to them:
// every NURBS pole grid in the export is exactly bilinear, which is the
// doubly-ruled patch `loft` builds from the same four corners — matched to
// 1.2e-4 mm, the export's own vertex scatter. Keeping the twist through
// ThruSections needed a kernel-wrapper change, recorded in
// vendor/opencascade/PARCAD-CHANGES.md. The document's other body is absent
// from the export, so there is nothing to hold a recreation of it to.

const BAR = 10; // square section of one bar
const SIDE = 115; // side of the triangle the three bar axes draw
const INSET = 9; // how far a bar's end square stops short of its axis vertex

const SIN60 = Math.sqrt(3) / 2;
const AXIS_RADIUS = SIDE / (2 * Math.sqrt(3)); // inradius of the axis triangle: where a bar lies
const HALF_SPAN = SIDE / 2 - INSET; // half the twisted length of one bar

const square = [
  [BAR / 2, BAR / 2],
  [-BAR / 2, BAR / 2],
  [-BAR / 2, -BAR / 2],
  [BAR / 2, -BAR / 2],
];
const turned = ([x, y]) => [-y, x];

// A loft pairs section corners by index, so listing the far square one corner
// along is the whole twist.
const bar = loft([
  { z: -HALF_SPAN, outline: square },
  { z: HALF_SPAN, outline: square.map(turned) },
])
  .rotate("x", -90)
  .at(0, 0, -AXIS_RADIUS);

// The corner block, where a bar arriving at 60° meets the one leaving along
// +Y: each section carries straight past the vertex and is trimmed on the
// other. That region is not convex — the export draws the notch as two sliver
// faces — so it is two quads meeting on the arriving bar's inner wall, drawn
// in the ring's (y, z) plane.
const along = [0.5, SIN60]; // direction the arriving bar runs
const outward = [-SIN60, 0.5]; // its outward normal
const outerEnd = [INSET * along[0] + (BAR / 2) * outward[0], INSET * along[1] + (BAR / 2) * outward[1]];
const innerEnd = [INSET * along[0] - (BAR / 2) * outward[0], INSET * along[1] - (BAR / 2) * outward[1]];

const slide = (from, z) => [from[0] + ((z - from[1]) / along[1]) * along[0], z]; // along the bar, to height z
const tip = slide(outerEnd, -BAR / 2); // the ring's outer corner
const innerCut = slide(innerEnd, -BAR / 2); // where the two quads meet
const innerTop = slide(innerEnd, BAR / 2);

const arriving = [tip, outerEnd, innerEnd, innerCut];
const leaving = [innerCut, innerTop, [INSET, BAR / 2], [INSET, -BAR / 2]];

// extrude() runs along +Z, so each quad is written on (y, z) and swung to run
// through the thickness instead.
const prism = (quad) =>
  extrude(
    quad.map(([y, z]) => [-z, y]),
    BAR,
  ).rotate("y", 90);

const corner = union(prism(arriving), prism(leaving)).at(0, -SIDE / 2, -AXIS_RADIUS);

// A bar and a corner make a third of the ring; two rotations close it.
const third = union(corner, bar);
return union(third, third.rotate("x", 120), third.rotate("x", 240));

// UnTriangle v3 — recreated from the Fusion 360 export.
//
// An impossible-triangle sculpture: three straight bars of 10 mm square
// section whose axes draw an equilateral triangle, each bar twisting a
// quarter turn between its corners so the flat of one end arrives as the
// edge of the next.
//
// The document holds two solids, and its STEP export carries only one:
// Body12, whose numbers parcad's own STEP probe reads off the file
// (`parcad --probe-step`, or the probe_step_export tool). This script
// recreates that body, measured against the export:
//
//   volume   24800.00 mm^3 against the export's 24799.94   (+0.00024%)
//   area     11619.41 mm^2 against 11619.40                (+0.00005%)
//   bounding box  10.00 x 132.32 x 114.59 mm               (exact)
//   faces    30, and the same 30: 18 plane, 12 nurbs
//
// The other body — Body1, 31976.42 mm^3, the one this file's header used to
// record — is not in the export at all, so there is nothing to measure a
// recreation of it against. It differs by a 45° twist phase (its bounding box
// is 10*sqrt(2) wide) and carries three fillets. The header numbers before
// this recreation described a body the reference never contained.
//
// The walls are the part worth understanding. Fusion lofted each bar between
// two squares a quarter turn apart, and every wall in the export is a NURBS
// surface whose pole grid is exactly bilinear: a doubly-ruled patch fully
// determined by its four corners. parcad's default ruled `loft` builds the
// same patch from the same corners, so the surfaces here are not merely
// close to Fusion's — they are the same surfaces, measured to 1.2e-4 mm,
// which is the export's own vertex scatter. Getting the twist through OCCT
// needed one kernel-wrapper change: ThruSections' compatibility pass used to
// re-origin the section wires to *remove* twist, silently rebuilding this
// loft as a straight prism (see vendor/opencascade/PARCAD-CHANGES.md).
// Vertex pairing is by outline index, so listing the top square's outline a
// quarter turn on IS the twist.
//
// Fusion's own timeline (Extrude, Mirror, Loft, CircularPattern, Combine,
// six Drafts) is not reproduced move for move: the drafts left every plane
// normal exactly axis-aligned or at exactly 60°, so whatever they were for,
// the finished body is the union below. Fusion's stored bbox for Body12
// (10.0000 x 132.3209 x 114.5933) carries ~2e-4 mm of sketch scatter around
// the clean construction; the export's B-rep is what this script is held to.

const A = 10; // bar cross-section square
const SIDE = 115; // side of the triangle the three bar axes draw
const INSET = 9; // each bar's end square sits this far from its axis vertex

const S3 = Math.sqrt(3) / 2;
const R_IN = SIDE / (2 * Math.sqrt(3)); // axis-triangle inradius; bars sit here
const HALF = SIDE / 2 - INSET; // half-length of the twisted span

// One twisted bar, built along +Z and laid down along +Y at the bottom of
// the ring. The loft pairs section vertices by index, so the same square
// listed a quarter turn on twists every wall by one vertex — that is the
// sculpture.
const sq = [
  [A / 2, A / 2],
  [-A / 2, A / 2],
  [-A / 2, -A / 2],
  [A / 2, -A / 2],
];
const quarter = ([x, y]) => [-y, x];
const bar = loft([
  { z: -HALF, outline: sq },
  { z: HALF, outline: sq.map(quarter) },
])
  .rotate("x", -90)
  .at(0, 0, -R_IN);

// The corner block between an incoming bar (up-right at 60°) and the
// outgoing bar (along +Y): each bar's square section carried straight on
// past the vertex, trimmed where it meets the other bar. That region is not
// convex — the export shows the resulting notch as two sliver faces — so it
// is two convex quads in the ring's (y, z) plane, pushed through the
// thickness. Their shared edge lies on the incoming bar's inner wall plane.
const u = [0.5, S3]; // incoming bar direction
const n = [-S3, 0.5]; // its outward normal
const outerEnd = [INSET * u[0] + (A / 2) * n[0], INSET * u[1] + (A / 2) * n[1]];
const innerEnd = [INSET * u[0] - (A / 2) * n[0], INSET * u[1] - (A / 2) * n[1]];
const hit = (p, z) => [p[0] + ((z - p[1]) / u[1]) * u[0], z]; // along u to height z
const tip = hit(outerEnd, -A / 2); // the ring's outer corner
const innerCut = hit(innerEnd, -A / 2);
const innerTop = hit(innerEnd, A / 2);

const incoming = [tip, outerEnd, innerEnd, innerCut];
const outgoing = [innerCut, innerTop, [INSET, A / 2], [INSET, -A / 2]];

// extrude() runs along +Z; swing the prism to run through the thickness
// (+X), with the profile's coordinates landing on (y, z).
const prism = (quad) =>
  extrude(
    quad.map(([y, z]) => [-z, y]),
    A,
  ).rotate("y", 90);

const corner = union(prism(incoming), prism(outgoing)).at(0, -SIDE / 2, -R_IN);

// One bar plus one corner is a third of the ring; the pattern closes it.
// Union order follows the chain of contacts, as always.
const unit = union(corner, bar);
return union(unit, unit.rotate("x", 120), unit.rotate("x", 240));

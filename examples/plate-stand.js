// A vertical dinner-plate stand for a cupboard shelf, drawn so the pegs stop
// breaking off.
//
// The stand this replaces (makerworld.com/models/1199653, after printables
// 227795) holds each plate between two rows of thin hooked pegs that taper to
// a point. Printed standing up, a peg is a stack of layers lying across the
// direction a leaning plate pushes it, so the root takes its bending moment on
// the weakest plane it has, and the hooked tip is a thin overhang. Both broke.
//
// Three changes. The peg is a straight cone leaning outward, twelve
// millimetres at the root instead of about eight: bending strength goes with
// the cube of the diameter, so that is over three times stronger before the
// blend is counted. The root is blended into the base at 4 mm, which spreads
// the load over many layers instead of one line and takes the stress
// concentration out of the corner. And the tip is a fillet rather than a
// point. The original's sizes are read off its photographs, not its file.
//
// It prints in one piece, base down, no supports: a 10° lean is well inside
// the overhang limit, and the pegs are still layers stacked across the load,
// which no one-piece orientation avoids. Four walls make a Ø12 peg solid
// perimeter, and PETG holds its layers together better than PLA. A peg that
// must be stronger again is printed lying down and pressed into the base,
// which is a different part.

const plates = 8;
const pitch = 28;         // plate to plate; vertical plates nest, so this is less than a plate is tall
const rowGap = 100;       // between the peg rows: see the note on plate sizes below
const margin = 12;        // base beyond the outermost peg, along the rows
const baseT = 6;
const cornerR = 10;

const pegRootD = 12;      // at the base surface
const pegTipD = 6;
const pegH = 40;          // above the base
const lean = 10;          // degrees outward, following the plate's edge
const rootBlend = 4;
const bury = 3;           // into the base, so the union has a seam to blend; short of the underside

const slotW = 16;
// Ends 5 mm short of the peg pads, whose reach is the root plus the blend.
const slotL = rowGap - 2 * (pegRootD / 2 + rootBlend + 5);

// A plate stands on the shelf through its slot and rests against a peg on
// each side, where its rim crosses the peg rows. The rows decide which plates
// that reaches, and the small ones are the test: a plate's rim rises steeply
// near its edge, so the rows must sit inside the smallest plate's chord. With
// the rows 100 mm apart, a 15 cm side plate meets the pegs 22 mm above the
// shelf and a 27 cm dinner plate 10 mm, against tips 45 mm up; a plate whose
// rim will not pass the slot stands 6 mm higher and still keeps 16 mm of peg.
// At 140 mm and a 20° lean, nearer the original, a 20 cm plate clears the
// tips and is not held at all, and at 120 mm a 15 cm one reaches the top 2 mm.
const L = plates * pitch + 2 * margin;
// Wide enough that the leaning tips stay over the base: the row, the lean's
// reach, the tip radius, and the same margin as the ends.
const W = 2 * (rowGap / 2 + pegH * Math.sin((lean * Math.PI) / 180) + pegTipD / 2 + margin);

// The base: rounded corners, a soft top edge, a small chamfer underneath so
// the first layer's elephant foot has nowhere to be.
const slab = box(L, W, baseT).at(0, 0, baseT / 2);
const base = slab
  .edges("|Z")
  .expect({ count: 4 })
  .fillet(cornerR)
  .edges(">Z")
  .expect({ count: 8 })
  .fillet(1.5)
  .edges("<Z")
  .expect({ count: 8 })
  .chamfer(0.8)
  .tag("base");

// One peg, standing on z = 0 and reaching `bury` below it; placed on the
// base's top face, so the buried end stops inside the base and never reaches
// the underside, which would give the union a second seam there. The cone is
// drawn from where it enters the base, so the root diameter above is the one
// at the surface rather than the one under it.
const taper = (pegRootD - pegTipD) / 2 / pegH;
const pegLen = pegH + bury;
const peg = cone(pegRootD / 2 + bury * taper, pegTipD / 2, pegLen)
  .at(0, 0, (pegH - bury) / 2)
  .edges(">Z")
  .expect({ count: 1 })
  .fillet(pegTipD / 2 - 0.5)
  .tag("peg");

// The rows. `plates` slots need `plates + 1` pegs a row; the front row leans
// toward +Y and the back row toward -Y, and rotating about X by a negative
// angle is what carries +Z toward +Y (see Shape.rotate).
const stations = Array.from({ length: plates + 1 }, (_, i) => (i - plates / 2) * pitch);
const front = stations.map((x) => peg.rotate("x", -lean).at(x, rowGap / 2, baseT));
const back = stations.map((x) => peg.rotate("x", lean).at(x, -rowGap / 2, baseT));

// Unioned flat rather than as two pre-fused rows: pegs do not touch each
// other, and a fuse that has to bridge eighteen separate lumps at once is the
// failure docs/GOTCHAS.md records for pipe bends.
const body = union(base, ...front, ...back, { blend: rootBlend }).tag("body");

// A slot under each plate, ends rounded. The cutter runs a millimetre past
// both faces of the base, as every cutter here must.
const slotEnd = (slotL - slotW) / 2;
const slot = union(
  box(slotW, slotL - slotW, baseT + 2),
  cylinder(slotW / 2, baseT + 2).at(0, slotEnd),
  cylinder(slotW / 2, baseT + 2).at(0, -slotEnd),
).at(0, 0, baseT / 2);
const slotAt = Array.from({ length: plates }, (_, i) => (i - (plates - 1) / 2) * pitch);
const slotted = body
  .cut(...slotAt.map((x) => slot.at(x, 0, 0)))
  .tag("slotted");

// Ease the top rim of every slot so a plate rim slides in rather than catching.
return slotted
  .edges({ generatedBy: "slotted", adjacentTo: { faceNormal: "+z" } })
  .expect({ count: plates * 4 })
  .chamfer(0.8)
  .tag("slot_rims");

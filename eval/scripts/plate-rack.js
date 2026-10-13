// Plate stand for 8 dinner plates

// Base dimensions
const baseW = 240;
const baseD = 240;
const baseH = 6;
const cornerRad = 5;

// Slot dimensions
const slotW = 16;
const slotSpacing = 28;
const numSlots = 8;

// Peg dimensions
const pegR1 = 6;      // radius at base (12 mm diameter)
const pegR2 = 3;      // radius at tip (6 mm diameter)
const pegLen = 40;
const pegLean = 10;   // lean angle in degrees
const pegBlend = 1.5; // blend radius
const pegBury = 3;    // bury pegs 3 mm into base

// Peg row parameters
const pegRowDist = 100;

// Create base plate with rounded top edges
let stand = box(baseW, baseD, baseH)
  .at(0, 0, baseH / 2)
  .edges(">Z")
  .fillet(cornerRad);

// Cut slots - 8 slots, 28 mm apart, centered
const slotStart = -(numSlots - 1) * slotSpacing / 2;
for (let i = 0; i < numSlots; i++) {
  const slotX = slotStart + i * slotSpacing;
  const slotCut = box(slotW, baseD + 2, baseH + 1)
    .at(slotX, 0, baseH / 2);
  stand = stand.cut(slotCut);
}

// Peg positions and creation
const pegStart = slotStart - slotSpacing / 2;
const numPegs = numSlots + 1;

const createPeg = (x, y, leanDir) => {
  let peg = cone(pegR1, pegR2, pegLen);
  peg = peg.rotate("x", pegLean * leanDir);
  // Bury pegs 3 mm into the base so they overlap
  peg = peg.at(x, y, baseH - pegBury + pegLen / 2);
  return peg;
};

// Add front row of pegs
for (let i = 0; i < numPegs; i++) {
  const pegX = pegStart + i * slotSpacing;
  const peg = createPeg(pegX, -pegRowDist / 2, 1);
  stand = stand.union(peg, { blend: pegBlend });
}

// Add back row of pegs
for (let i = 0; i < numPegs; i++) {
  const pegX = pegStart + i * slotSpacing;
  const peg = createPeg(pegX, pegRowDist / 2, -1);
  stand = stand.union(peg, { blend: pegBlend });
}

return stand;

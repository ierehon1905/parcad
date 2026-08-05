// A pillow block for a 20 mm bore ball bearing (a UCP-204 style housing,
// simplified to the shapes a machinist would actually cut from bar stock).
//
// The bore axis runs along Y, so the boss is a cylinder rotated 90° about X.
// Shaft height — bore centreline above the mounting face — is the dimension
// this part exists to hold, so it is written once and everything else follows.

const shaftHeight = 25;    // bore centreline above the base underside
const baseW = 90;          // along X
const baseD = 32;          // along Y, the bearing width
const baseT = 12;          // base plate thickness
const bossDia = 47;        // bearing outer diameter housing
const bore = 20;
const boltSpacing = 66;    // between mounting hole centres
const boltDia = 8.5;       // clearance for M8

const base = box(baseW, baseD, baseT)
  .at(0, 0, baseT / 2)
  .tag("base");

// The boss reaches down into the base rather than butting onto its top face.
// A blended union between two solids that only touch on a face crashes the
// kernel — see docs/DSL_GAPS.md — and a real housing is one casting anyway.
const boss = cylinder(bossDia / 2, baseD)
  .rotate("x", 90)
  .at(0, 0, shaftHeight)
  .tag("boss");

const body = union(base, boss, { blend: 4 }).tag("body");

// Overlength through the whole housing; a bore that stops exactly on the face
// leaves a zero-thickness sliver for the boolean to trip over.
const bearingBore = cylinder(bore / 2, baseD * 3).rotate("x", 90).at(0, 0, shaftHeight);

// Slots, not holes, in a real pillow block — alignment is set at assembly.
// A slot is a box with two cylinders on its ends: the DSL has no 2D sketch,
// which is fine here because the shape is genuinely three primitives.
const slotEnds = boltDia / 2;
const slotTravel = 6;
const slot = union(
  box(slotTravel, boltDia, baseT * 3),
  cylinder(slotEnds, baseT * 3).at(-slotTravel / 2, 0),
  cylinder(slotEnds, baseT * 3).at(slotTravel / 2, 0),
);

const machined = body
  .cut(
    bearingBore,
    slot.at(-boltSpacing / 2, 0, baseT / 2),
    slot.at(boltSpacing / 2, 0, baseT / 2),
  )
  .tag("machined");

// Both bore rims get a lead-in so the bearing presses in square. Two, not six:
// `role: "hole"` matches complete circular rims, and a slot end is a pair of
// arcs joined to straight sides, not a circle. It also keeps the boss's own
// outside rim — same curve, same radius, convex — out of the selection.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole" })
  .expect({ count: 2 })
  .chamfer(0.8)
  .tag("press_fit_lead_in");

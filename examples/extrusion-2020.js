// A 200 mm length of 20x20 T-slot aluminium extrusion.
//
// The profile is the standard one every 3D printer frame is built from: a
// 20 mm square with a 6 mm slot opening on each face, widening to an 11 mm
// channel that the T-nut sits in, and a 4.2 mm core hole for a self-tapping
// end screw.
//
// It is written as one side's cuts, placed four times by rotating the *cutter*
// rather than the body — `around(slot, 4)`. The profile is genuinely four-fold
// symmetric, so that is exact rather than four placements that could drift
// apart under an edit.

const size = 20;
const length = 200;
const slotOpening = 6.2;   // the gap at the face
const openingDepth = 2.0;  // how deep before it widens
const channel = 11.0;      // T-nut chamber width
const channelDepth = 6.0;  // from the face to the back of the chamber
const core = 4.2;          // end-screw hole

const bar = box(size, size, length).tag("bar");

// One face's cut, made from the outside in: the visible gap, then the chamber
// behind it. Both are overlength along the extrusion axis so nothing depends
// on a tool ending exactly on the end faces.
const opening = box(slotOpening, openingDepth * 2, length * 1.2)
  .at(0, size / 2 - openingDepth / 2 + openingDepth / 2, 0);

const chamber = box(channel, channelDepth - openingDepth, length * 1.2)
  .at(0, size / 2 - openingDepth - (channelDepth - openingDepth) / 2, 0);

const slot = union(opening, chamber);

// Four identical slots. Rotating the cutter about the extrusion axis is exact:
// the profile is genuinely four-fold symmetric, so nothing here is a placement
// approximation that could drift.
const slots = around(slot, 4).tag("slots");

const coreHole = cylinder(core / 2, length * 1.2).tag("core_hole");

const profile = bar.cut(slots, coreHole).tag("profile");

// The four outside corners are broken on a real extrusion — the die has a
// radius there, and a sharp 20x20 corner is a hazard on a frame.
//
// One selector per corner, because "|Z" alone is every edge running along the
// extrusion, and this profile has thirty-seven of them: each slot contributes
// its own. Adding the two extrema per corner narrows it to the one edge meant.
// docs/DSL_GAPS.md notes what is missing — a way to say "the outermost of
// these" without enumerating corners by hand.
const corners = [">X and >Y", ">X and <Y", "<X and >Y", "<X and <Y"];

return corners
  .reduce(
    (shape, corner) => shape.edges(`${corner} and |Z`).expect({ count: 1 }).fillet(1),
    profile,
  )
  .tag("corner_radii");

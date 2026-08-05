// An M3 hex standoff, 5.5 mm across the flats, 20 mm long, bored 2.5 mm for
// a tapped thread.
//
// There is no prism primitive, so the hexagon is the intersection of three
// slabs at 60° — the classic construction, and exact: each slab contributes
// one pair of opposite flats. Across-flats is the slab thickness, so 5.5 here
// is the wrench size, not the across-corners diameter (6.35).

const acrossFlats = 5.5;
const length = 20;
const tapDrill = 2.5;

// Each slab must be wide enough that only its own two flats can bound the
// result: across-corners is acrossFlats * 2 / sqrt(3), so 2x is ample.
const slab = box(acrossFlats, acrossFlats * 2, length);

const hex = intersect(slab, slab.rotate("z", 60), slab.rotate("z", 120)).tag("hex");

const bore = cylinder(tapDrill / 2, length * 2).tag("tap_drill");

const drilled = hex.cut(bore).tag("drilled");

// Both end faces get a lead-in chamfer on the bore. Selecting by role and
// curve picks exactly the two rims and never a flat-to-flat vertical edge.
return drilled
  .edges({ generatedBy: "drilled", curve: "circle", role: "hole" })
  .expect({ count: 2 })
  .chamfer(0.4)
  .tag("thread_lead_in");

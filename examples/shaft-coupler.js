// A rigid set-screw shaft coupler: 8 mm motor shaft to 10 mm leadscrew.
//
// The two bores are different sizes and meet in the middle, which is the point
// of the part — and it is also the thing that makes it easy to get wrong. The
// bores are drilled to a depth each, not through, so there is a web of metal
// between them; that web is what stops the screw from being driven into the
// motor bearing.

const od = 25;
const length = 30;
const boreA = 8;      // motor side
const boreB = 10;     // leadscrew side
const boreDepth = 13; // each, leaving a 4 mm web at the centre
const setScrew = 4.2; // tap drill for M5 grub screws

const body = cylinder(od / 2, length).tag("body");

// Each bore is modelled overlength and placed so its open end sticks out past
// the coupler face: a cutting tool that stops exactly on a face leaves a
// zero-thickness sliver, which is a classic source of boolean failures.
const over = 5;
const motorBore = cylinder(boreA / 2, boreDepth + over)
  .at(0, 0, -length / 2 + (boreDepth + over) / 2 - over)
  .tag("motor_bore");

const screwBore = cylinder(boreB / 2, boreDepth + over)
  .at(0, 0, length / 2 - (boreDepth + over) / 2 + over)
  .tag("screw_bore");

// Two grub screws per side, at 90° to each other, so the shaft is pinched
// rather than pushed off centre. Radial holes are cylinders rotated onto X
// and Y and moved along the axis.
const grub = cylinder(setScrew / 2, od * 1.5);
const grubs = union(
  grub.rotate("y", 90).at(0, 0, -length / 2 + boreDepth / 2),
  grub.rotate("x", 90).at(0, 0, -length / 2 + boreDepth / 2),
  grub.rotate("y", 90).at(0, 0, length / 2 - boreDepth / 2),
  grub.rotate("x", 90).at(0, 0, length / 2 - boreDepth / 2),
);

const machined = body.cut(motorBore, screwBore, grubs).tag("machined");

// Lead-in on the leadscrew bore so a shaft enters without scraping. One edge:
// naming the +Z face normal excludes the motor bore's rim at the other end,
// and the grub screw holes contribute nothing either way — they break out
// into the bores as arcs, and their outer rims lie on the round outside face,
// whose normal is not an axis direction.
return machined
  .edges({
    generatedBy: "machined",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "+z" },
  })
  .expect({ count: 1 })
  .chamfer(0.8)
  .tag("screw_side_lead_in");

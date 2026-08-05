// A NEMA 17 stepper motor mount: an L-bracket whose face plate carries the
// standard motor pattern.
//
// The NEMA 17 interface is fixed by the standard and is the whole reason the
// part has these numbers: four M3 holes on a 31.0 mm square, and a 22 mm pilot
// boss that takes the load off the screws. Everything else is stock sizes.

const plateT = 6;
const faceW = 50;
const faceH = 50;
const footD = 45;         // how far the foot reaches back
const boltSquare = 31.0;  // NEMA 17
const boltDia = 3.4;      // clearance for M3
const pilot = 23;         // clearance around the 22 mm motor boss
const mountHole = 5.5;    // clearance for M5 into the frame

// The face plate stands in the XZ plane with its motor face on y = 0; the foot
// lies flat. Both are placed from their own centres, because primitives here
// are centred on the origin by construction.
const face = box(faceW, plateT, faceH)
  .at(0, plateT / 2, faceH / 2)
  .tag("face");

const foot = box(faceW, footD, plateT)
  .at(0, footD / 2, plateT / 2)
  .tag("foot");

// Gussets are what make an L-bracket stiff; without them the face plate hinges
// about the seam under belt tension. A triangular web is a box rotated 45° and
// intersected with a slab that gives it its thickness.
//
// They sit outboard at x = ±20 deliberately: a single central gusset would run
// straight through the 23 mm pilot bore, turning the bore's back rim into a
// pair of arcs and quietly breaking the rim count asserted at the end.
const leg = 20;   // how far the web runs up the face and back along the foot
const gusset = intersect(
  // The corner being braced: 4 mm thick, sitting in the positive quadrant.
  box(4, leg, leg).at(0, leg / 2, leg / 2),
  // The 45° hypotenuse. A cube rotated 45° about X is bounded by y + z = h,
  // where h is half its diagonal, so sizing it leg * sqrt(2) puts that plane
  // exactly through the two leg ends.
  box(leg * 2, leg * Math.SQRT2, leg * Math.SQRT2).rotate("x", 45),
);

// The seam blend is taken between the two plates alone, then the gussets are
// unioned on unblended. A blend across all four solids at once asks OCCT to
// fillet edges that the gussets land exactly on, and it aborts rather than
// refusing — see docs/DSL_GAPS.md.
const shell = union(face, foot, { blend: 2 }).tag("shell");

const body = union(shell, gusset.at(-20, 0, 0), gusset.at(20, 0, 0)).tag("body");

// The motor pattern, drilled along Y through the face plate. grid() gives the
// four corners of the bolt square; they are lifted to the bore centre height.
const motorHole = cylinder(boltDia / 2, plateT * 4).rotate("x", 90);
const motorHoles = grid(2, 2, boltSquare, boltSquare).map(([x, z]) => [
  x,
  0,
  z + faceH / 2,
]);

const pilotBore = cylinder(pilot / 2, plateT * 4).rotate("x", 90).at(0, 0, faceH / 2);

const frameHoles = repeat(cylinder(mountHole / 2, plateT * 4), [
  [-16, footD - 10],
  [16, footD - 10],
]);

const drilled = body
  .cut(pilotBore, repeat(motorHole, motorHoles), frameHoles)
  .tag("drilled");

// Deburr the motor face only: the four screw holes and the pilot bore, on the
// one face that has to sit flat against the motor. Naming the face normal is
// what limits it to five — the same five holes have rims on the back face too,
// and those are none of this operation's business.
return drilled
  .edges({
    generatedBy: "drilled",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "-y" },
  })
  .expect({ count: 5 })
  .chamfer(0.5)
  .tag("motor_face_deburr");

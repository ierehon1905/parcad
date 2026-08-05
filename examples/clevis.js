// A threaded rod-end clevis: a shank with spanner flats, and a two-armed fork
// for a 10 mm pin.
//
// This is the part that shows what `mirror` is for. The fork is symmetric, so
// one arm is authored and the other is its reflection — before mirror existed
// the second arm was a second copy of the same arithmetic with the signs
// changed by hand, which is how a part ends up with one arm 13 mm thick and the
// other 13.5. `ngon` is the other new one: spanner flats are quoted across the
// flats, and that is what the call says.

const shankDia = 20;
const shankLen = 26;
const tapDrill = 10.2;   // M12 x 1.75
const threadDepth = 20;
const flats = 17;        // across the flats, a 17 mm spanner
const flatsLen = 12;

const crownW = 40;
const crownD = 24;
const crownT = 8;

const armT = 13;         // each arm
const gap = 14;          // the tongue that fits between them
const armH = 26;
const pinDia = 10;
const pinZ = crownT + armH - 9;

// The shank hangs below the crown. It runs up past z = 0 so the crown has
// something to sit *in* rather than *on*: a blended union of two solids that
// only touch on a face aborts inside OCCT (docs/GOTCHAS.md).
const shank = cylinder(shankDia / 2, shankLen + 3).at(0, 0, (-shankLen + 3) / 2);

// Spanner flats: a band of everything outside the hexagon, taken off the round
// shank. The hexagon is the thing being kept, so the cutter is the band with
// the hexagon removed from it — which reads as what a machinist would say.
const band = box(shankDia * 2, shankDia * 2, flatsLen)
  .at(0, 0, -shankLen + flatsLen / 2)
  .cut(ngon(6, flats, flatsLen, { across: "flats" }).at(0, 0, -shankLen + flatsLen / 2));

const crown = box(crownW, crownD, crownT).at(0, 0, crownT / 2);

// One arm, authored on +X, and its reflection. `union` is separate from
// `mirror` on purpose: the reflection alone is the left-hand part.
const arm = box(armT, crownD, armH).at(gap / 2 + armT / 2, 0, crownT + armH / 2);
const fork = union(arm, arm.mirror("x"));

const body = union(shank, crown, fork).tag("body");

// The tapped hole is drawn as its tap drill — there is no thread op, and a
// stack of tori pretending to be one is the approximation this project refuses.
// It starts below the end face so the cutter crosses it rather than ending on
// it, and reaches the called-out depth.
const thread = cylinder(tapDrill / 2, threadDepth + 4)
  .at(0, 0, -shankLen - 4 + (threadDepth + 4) / 2)
  .tag("tap_drill");

const turned = body.cut(band, thread).tag("turned");

// Across both arms, and out the far side of each.
const pin = cylinder(pinDia / 2, crownW * 2).rotate("y", 90).at(0, 0, pinZ);
const machined = turned.cut(pin).tag("machined");

// Four pin-hole rims: two arms, two faces each, and no others because the bore
// is its own cut. The count is the assertion that the pin hole still goes all
// the way through — an arm moved outboard of it would leave two.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole" })
  .expect({ count: 4 })
  .chamfer(0.5)
  .tag("pin_lead_in");

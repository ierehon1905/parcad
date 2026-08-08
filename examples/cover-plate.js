// A bolted cover plate with a turned spigot and countersunk screws.
//
// This is the part that could not be modelled at all until `revolve` existed:
// both the spigot's taper and the screw countersinks are cones, and a cone is a
// revolved triangle. Everything here is still exact — a taper authored as a
// section is the shape a lathe leaves, not an approximation of it.

const plate = 90;      // square
const plateT = 8;
const spigotDia = 40;  // where it enters the housing bore
const spigotTip = 36;  // lead-in taper at the far end
const spigotH = 14;
const bore = 20;
const screw = 5.5;     // clearance for M5
const head = 10.4;     // M5 countersunk head diameter
const boltSquare = 70;

const body = box(plate, plate, plateT).at(0, 0, plateT / 2).tag("plate");

// The spigot hangs below the plate and tapers, so it finds the bore on the way
// in. `cone` takes radii, like `cylinder` — the diameters above are halved here
// rather than at the top, because a drawing calls out the diameter and it is
// worth having the numbers in the file match it. It is centred on the origin
// like every other primitive, so it is placed by its middle.
//
// It reaches up into the plate rather than butting onto its underside: a
// blended union between solids that only touch on a face is refused
// (docs/GOTCHAS.md), and a cast cover has a root radius there anyway.
const spigot = cone(spigotTip / 2, spigotDia / 2, spigotH)
  .at(0, 0, -spigotH / 2 + 2)
  .tag("spigot");

const casting = union(body, spigot, { blend: 3 }).tag("casting");

// A countersink is a cone too, and this one is sized the way a drawing calls it
// out: by head diameter and included angle. 90° is the metric standard.
const sink = countersink(head, 90);

const screwHoles = grid(2, 2, boltSquare, boltSquare).flatMap(([x, y]) => [
  cylinder(screw / 2, plateT * 4).at(x, y),
  // The countersink sits on the top face, which is where the head lands.
  sink.at(x, y, plateT),
]);

const machined = casting
  .cut(cylinder(bore / 2, (plateT + spigotH) * 4), ...screwHoles)
  .tag("machined");

// The four countersinks meet the top face on a circle of the head diameter, and
// the bore rim is a fifth. Deburring all five in one call is what a machinist
// would do, and the count is the assertion that all four screws are still
// there — a hole lost to an edit fails here rather than in assembly.
//
// No `role: "hole"` here, unlike the other examples: that term does not
// recognise a *conical* opening, so adding it drops the four countersink rims
// and leaves only the bore. See docs/DSL_GAPS.md.
return machined
  .edges({
    generatedBy: "machined",
    curve: "circle",
    at: { z: "max" },
  })
  .expect({ count: 5 })
  .chamfer(0.4)
  .tag("top_face_deburr");

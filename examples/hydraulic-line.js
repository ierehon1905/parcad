// A bent hydraulic line with a flare fitting boss at each end, and the O-ring
// groove that seals one of them.
//
// `pipe` routes 12 mm tube through 20 mm bends — straight runs and partial
// tori, both exact, which is as much of "sweep" as can be built honestly here.
// What is still not expressible is a path that curves continuously: a spline
// has no exact distance field, so it is not offered rather than fitted.
//
// The inlet boss carries its O-ring groove. That used to be impossible: the
// coaxial seams a torus cut leaves in a cylinder wall segfaulted
// `UnifySameDomain`, and this part went without and said so. OCCT 8.0.1 fixed
// it; `eval/cases/torus-gland.json` is what proves the fix is still there.
//
// The groove is round-bottomed, because a torus cut is a circle in section and
// that is the only section available. A catalogue gland is rectangular and
// *wider* than the cord, which no torus can cut — see docs/DSL_GAPS.md, "arcs
// in a section". So this is a seat the ring is stretched over and drops into,
// which is how an external groove is assembled anyway, and not a claim to a
// standard section.

const tube = 12;
const wall = 1.5;
const bend = 20;      // centreline bend radius, 1.67 x diameter
const bossDia = 24;
const bossLen = 14;
const cord = 2;         // O-ring cord diameter, 2 mm metric
const glandDepth = 1.5; // 0.5 mm of the cord stands proud to be squeezed
const glandFromEnd = 4; // back from the free end, clear of the fitting's lead-in

// The route: out of the pump, along, up, and across to the manifold. Each
// corner gets the same bend, which is what one tool setting gives you.
const route = [
  [0, 0, 0],
  [70, 0, 0],
  [70, 55, 0],
  [70, 55, 40],
  [130, 55, 40],
];

const line = pipe(route, tube, { bend }).tag("line");

// The groove is written from the cord, so changing the ring moves the whole
// feature: the torus centreline sits one cord-radius outside the groove
// bottom, and that bottom is `glandDepth` under the boss surface.
const gland = torus(bossDia / 2 - glandDepth + cord / 2, cord / 2)
  .rotate("y", 90)
  .at(glandFromEnd, 0, 0)
  .tag("gland");

// A boss at each end, over the tube, for the fitting to thread into. Each sits
// on the run it belongs to, so it is placed by the route's own numbers.
const inlet = cylinder(bossDia / 2, bossLen)
  .rotate("y", 90)
  .at(bossLen / 2, 0, 0)
  .cut(gland);
const outlet = cylinder(bossDia / 2, bossLen)
  .rotate("y", 90)
  .at(130 - bossLen / 2, 55, 40);

const body = union(line, inlet, outlet).tag("body");

// The bore is the same route at the wall diameter — one `pipe` call cannot be
// reused at two sizes, but the route can, and that is the part that must not
// drift. Both ends are pushed 2 mm past the tube ends: a cutter that stops
// exactly on the face it exits leaves a zero-thickness sliver, and here it
// killed the kernel outright rather than producing a bad solid.
const past = 2;
const boreRoute = [
  [-past, 0, 0],
  ...route.slice(1, -1),
  [130 + past, 55, 40],
];
const bore = pipe(boreRoute, tube - 2 * wall, { bend }).tag("bore");

const machined = body.cut(bore).tag("machined");

// Both tube ends, where the fitting seats. Two rims, and the count is the
// assertion that the bore still runs the whole route: a bend radius that stops
// fitting would break the chain and leave a different number.
return machined
  .edges({ generatedBy: "machined", curve: "circle", role: "hole" })
  .expect({ count: 2 })
  .chamfer(0.5)
  .tag("seat_chamfer");

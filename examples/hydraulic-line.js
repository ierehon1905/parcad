// A bent hydraulic line with a flare fitting boss at each end, and the O-ring
// groove that seals one of them.
//
// `pipe` routes 12 mm tube through 20 mm bends — straight runs and partial
// tori, both exact, which is as much of "sweep" as can be built honestly here.
// What is still not expressible is a path that curves continuously: a spline
// has no exact distance field, so it is not offered rather than fitted.
//
// The inlet boss should carry an O-ring gland, and a torus is the exact shape
// of one. It is not here: a coaxial torus groove in a cylinder segfaults the
// kernel on the way out of the boolean, which `eval/cases/torus-gland.json`
// holds as a known defect and docs/GOTCHAS.md explains. A square-bottomed
// groove would build, and would not be what an O-ring seals against, so this
// part goes without until the crash is fixed.

const tube = 12;
const wall = 1.5;
const bend = 20;      // centreline bend radius, 1.67 x diameter
const bossDia = 24;
const bossLen = 14;

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

// A boss at each end, over the tube, for the fitting to thread into. Each sits
// on the run it belongs to, so it is placed by the route's own numbers.
const inlet = cylinder(bossDia / 2, bossLen)
  .rotate("y", 90)
  .at(bossLen / 2, 0, 0);
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

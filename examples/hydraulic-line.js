// A bent hydraulic line with a flare fitting boss at each end, and the O-ring
// groove that seals one of them.
//
// `pipe` routes 12 mm tube through 20 mm bends — straight runs and partial
// tori, both exact, which is what a tube bender makes. A hose that curves
// continuously would be `pipe({ spline: [...] }, d)` instead.
//
// The inlet boss carries its O-ring gland, drawn the way a catalogue draws
// one: a rectangular section wider than the cord, with its bottom corners
// radiused. It was a round-bottomed torus cut until sections could hold a
// rounded corner, because a torus is a circle in section and nothing else
// was — see docs/DSL_GAPS.md, "arcs in a section". The coaxial seams a cut
// like this leaves in a cylinder wall once segfaulted `UnifySameDomain`;
// `eval/cases/torus-gland.json` holds that fix.

const tube = 12;
const wall = 1.5;
const bend = 20;      // centreline bend radius, 1.67 x diameter
const bossDia = 24;
const bossLen = 14;
const cord = 2;          // O-ring cord diameter, 2 mm metric
const glandDepth = 1.5;  // 25% squeeze: 0.5 mm of the cord stands proud
const glandWidth = 2.7;  // wider than the cord, so it has room to deform
const glandRadius = 0.3; // bottom corner radius, the catalogue's 0.2–0.4
const glandFromEnd = 4;  // back from the free end, clear of the fitting's lead-in

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

// The cutter is the gland's section in (radius, z), revolved: its floor
// `glandDepth` under the boss surface with both floor corners rounded, and
// its outer side 1 mm proud of the boss so the cut breaks the surface
// cleanly. Turned onto the boss's X axis afterwards.
const floor = bossDia / 2 - glandDepth;
const proud = bossDia / 2 + 1;
const gland = revolve([
  { at: [floor, -glandWidth / 2], round: glandRadius },
  [proud, -glandWidth / 2],
  [proud, glandWidth / 2],
  { at: [floor, glandWidth / 2], round: glandRadius },
])
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

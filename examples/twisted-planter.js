// Twisted planter and its drip saucer, printed as two parts.
//
// The planter is a six-point star lofted through nine sections, each wider
// and turned further than the last, so its walls come out as twisted facets.
// The saucer is a revolved dish with spiral arms for the planter to stand on.

const points = 6;
const height = 90;
const twist = 60; // degrees, base to rim
const layers = 8;
const wall = 3; // inset across the star; the facets' own wall measures 2.0 mm
const floor = 3;

// At height z: the star's tip radius, flaring fastest near the base, and its turn.
const radiusAt = (z) => 34 + 18 * Math.sin((z / height) * (Math.PI / 2));
const turnAt = (z) => (z / height) * twist;

function star(z, inset) {
  const outline = Array.from({ length: 2 * points }, (_, i) => {
    const r = (i % 2 === 0 ? 1 : 0.78) * radiusAt(z) - inset;
    const angle = ((i * 180) / points + turnAt(z)) * (Math.PI / 180);
    return [r * Math.cos(angle), r * Math.sin(angle)];
  });
  return { z, outline };
}

// Section heights shared by the outside and the cavity, so their facets stay parallel.
const levels = Array.from({ length: layers + 1 }, (_, i) => (i * height) / layers);

const outside = loft(levels.map((z) => star(z, 0)));
const cavity = loft([floor, ...levels.filter((z) => z > floor), height + 1].map((z) => star(z, wall)));

const drain = cylinder(2.5, 3 * floor);

const planter = outside
  .cut(
    cavity, // 1 mm past the rim, so the top opens cleanly
    drain,
    ...polar(6, 14).map(([x, y]) => drain.at(x, y)),
  )
  .tag("planter");

// Wide enough that the 34 mm star base sits inside with room for runoff.
const saucerRadius = 42;
const saucerFloor = 2;

// The planter stands on two rounded rings, 3.5 mm proud of the floor: past the
// ~2.7 mm gap water bridges by surface tension, so its base drains instead of
// wicking. Notches in the rings, staggered, let the water out.
const ringHeight = 3.5;
const ringWidth = 3;
const rings = [21, 31]; // the inner one clear of the drains at 14 mm

// A ring in section: straight sides with a semicircular top.
const ring = (r) => [
  [r + ringWidth / 2, saucerFloor],
  [r + ringWidth / 2, saucerFloor + ringHeight - ringWidth / 2],
  { through: [r, saucerFloor + ringHeight] },
  [r - ringWidth / 2, saucerFloor + ringHeight - ringWidth / 2],
  [r - ringWidth / 2, saucerFloor],
];

// A radial slot through a ring down to the floor, at the point it is aimed at.
const notch = (x, y) =>
  box(3 * ringWidth, 4, 2 * ringHeight)
    .rotate("z", (Math.atan2(y, x) * 180) / Math.PI)
    .at(x, y, saucerFloor + ringHeight);

const saucer = revolve([
  [0, 0],
  { at: [saucerRadius + 2, 0], round: 3 },
  [saucerRadius + 2, 12],
  [saucerRadius, 12],
  { at: [saucerRadius, saucerFloor], round: 2 },
  ...rings.slice().reverse().flatMap(ring),
  [0, saucerFloor],
])
  .cut(
    ...polar(6, rings[0]).map(([x, y]) => notch(x, y)), // in line with the drains
    ...polar(6, rings[1], { straddle: true }).map(([x, y]) => notch(x, y)),
  )
  .tag("saucer");

return {
  planter: planter.at(0, 0, saucerFloor + ringHeight),
  saucer,
};

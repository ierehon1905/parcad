// Twisted planter and its drip saucer, printed as two parts.
//
// The planter is a six-point star lofted through nine sections, each wider
// and turned further than the last, so its walls come out as twisted facets.
// The saucer is a dish of smooth bumps in rings of 1, 6, 12 and 18, which the
// planter stands on.

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
    ...polar(6, 14, { straddle: true }).map(([x, y]) => drain.at(x, y)), // between the saucer's first ring of bumps
  )
  .tag("planter");

// Wide enough that the 34 mm star base sits inside with room for runoff.
const saucerRadius = 42;
const saucerFloor = 2;
const bumpRadius = 5.4;
const bumpHeight = 2.4;
const ringSpacing = 11;
const ringCount = 3;

// Smootherstep: no slope and no curvature where a bump leaves the floor or at its top.
const ease = (x) => x * x * x * (x * (6 * x - 15) + 10);

function bump() {
  const z = (r) => saucerFloor + bumpHeight * (1 - ease(r / bumpRadius));
  const profile = [0.92, 0.82, 0.7, 0.58, 0.46, 0.34, 0.2].map((t) => [t * bumpRadius, z(t * bumpRadius)]);
  return revolve([
    [0, 0],
    [bumpRadius, 0],
    [bumpRadius, saucerFloor],
    { spline: profile, start: [-1, 0], end: [-1, 0] },
    [0, saucerFloor + bumpHeight],
  ]);
}

const dish = revolve([
  [0, 0],
  { at: [saucerRadius + 2, 0], round: 3 },
  [saucerRadius + 2, 11],
  { through: [saucerRadius + 1, 12] },
  [saucerRadius, 11],
  { at: [saucerRadius, saucerFloor], round: 2 },
  [0, saucerFloor],
]);

// One bump in the middle, then rings of 6, 12, 18: each ring's circumference grows
// by 2π·spacing, so six more keeps every bump the same distance from its neighbours.
// Even rings sit half a step round from the odd ones.
const one = bump();
const bumps = [one];
for (let ring = 1; ring <= ringCount; ring++) {
  const count = 6 * ring;
  const start = ring % 2 === 0 ? 180 / count : 0;
  for (const [x, y] of polar(count, ring * ringSpacing, { start })) bumps.push(one.at(x, y));
}

const saucer = dish.union(...bumps);

return {
  planter: planter.at(0, 0, saucerFloor + bumpHeight),
  saucer,
};

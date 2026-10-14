// Twisted planter and its drip saucer, printed as two parts.
//
// The planter is a six-point star lofted through nine sections, each wider
// and turned further than the last, so its walls come out as twisted facets.
// The saucer is a dish of domed bumps in rings of 1, 6, 12 and 18, each eased
// into the floor with a curvature-continuous blend, which the planter stands on.

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
const bumpHeight = 2.4;
const ringSpacing = 11;
const ringCount = 3;

// Each bump is a dome: the cap of a sphere whose centre is at the saucer's
// underside, so it stays inside the floor. Its crease with the floor is eased
// afterwards with smooth(), a blend with no jump in curvature at either end.
const domeRadius = saucerFloor + bumpHeight;
const easeRadius = 2; // the blend's foot lands 5.0 mm from the bump's axis

function bump() {
  const foot = Math.sqrt(domeRadius ** 2 - saucerFloor ** 2); // where the sphere crosses the floor
  const shoulder = domeRadius / Math.SQRT2;
  return revolve([
    [0, 0],
    [foot, 0],
    [foot, saucerFloor],
    { through: [shoulder, shoulder] },
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

// Every bump's root is the saucer's only sharp concave circle; the dish's own
// corners are rounds, which read as smooth.
const saucer = dish
  .union(...bumps)
  .edges({ dihedral: "concave", curve: "circle" })
  .expect({ count: 1 + 6 * ((ringCount * (ringCount + 1)) / 2) })
  .smooth(easeRadius);

return {
  planter: planter.at(0, 0, saucerFloor + bumpHeight),
  saucer,
};

// Twisted planter and its drip saucer, printed as two parts.
//
// The planter is a six-point star lofted through nine sections, each wider
// and turned further than the last, so its walls come out as twisted facets.
// The saucer is one revolved section with rounded corners.

const points = 6;
const height = 90;
const twist = 60; // degrees, base to rim
const layers = 8;
const wall = 3; // inset across the star; the facets' own wall measures 2.0 mm
const floor = 3;
const footHeight = 2;

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
const foot = cylinder(4, footHeight).at(0, 0, -footHeight / 2);

const planter = outside
  .cut(
    cavity, // 1 mm past the rim, so the top opens cleanly
    drain,
    ...polar(6, 14).map(([x, y]) => drain.at(x, y)),
  )
  .union(...polar(3, 20, { start: 90 }).map(([x, y]) => foot.at(x, y)))
  .tag("planter");

// Wide enough that the 34 mm star base sits inside with room for runoff.
const saucerRadius = 42;
const saucerFloor = 2;
const saucer = revolve([
  [0, 0],
  { at: [saucerRadius + 2, 0], round: 3 },
  [saucerRadius + 2, 12],
  [saucerRadius, 12],
  { at: [saucerRadius, saucerFloor], round: 2 },
  [0, saucerFloor],
]).tag("saucer");

return {
  planter: planter.at(0, 0, saucerFloor + footHeight),
  saucer,
};

// The same tube, each rim filled flat and the three stitched: a closed shell,
// so a solid.
const tube = surfaceExtrude([[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }], 30, { closed: true });
const lid = tube.edges({ role: "boundary", at: { z: "max" } }).expect({ count: 2 }).patch();
const floor = tube.edges({ role: "boundary", at: { z: "min" } }).expect({ count: 2 }).patch();
return stitchSurfaces(tube, lid, floor, { solid: true });

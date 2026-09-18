// The same tube, each rim filled flat and the three stitched: a closed shell,
// so a solid. Each rim is one edge, though the section's two arcs make it two
// kernel pieces; see docs/GOTCHAS.md, "A seam cut the rim of a bead in two".
const tube = surfaceExtrude([[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }], 30, { closed: true });
const lid = tube.edges({ role: "boundary", at: { z: "max" } }).expect({ count: 1 }).patch();
const floor = tube.edges({ role: "boundary", at: { z: "min" } }).expect({ count: 1 }).patch();
return stitchSurfaces(tube, lid, floor, { solid: true });

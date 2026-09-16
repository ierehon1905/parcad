// Four walls as one closed extruded curve, top and bottom patched flat, and
// stitched: a 20 mm cube.
const walls = surfaceExtrude([[-10, -10], [10, -10], [10, 10], [-10, 10]], 20, { closed: true });
const top = walls.edges({ role: "boundary", at: { z: "max" } }).expect({ count: 4 }).patch();
const bottom = walls.edges({ role: "boundary", at: { z: "min" } }).expect({ count: 4 }).patch();
return stitchSurfaces(walls, top, bottom);

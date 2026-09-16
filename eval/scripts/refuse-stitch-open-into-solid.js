const tube = surfaceExtrude([[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }], 30, { closed: true });
return stitchSurfaces(tube, tube.edges({ role: "boundary", at: { z: "max" } }).patch(), { solid: true });

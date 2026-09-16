// The tube moved 2 mm along its outward normal: a tube of radius 12.
const tube = surfaceExtrude([[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }], 30, { closed: true });
return tube.offsetSurface(2);

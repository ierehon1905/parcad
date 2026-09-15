// A circle, as two arcs, lofted (ruled) to a point above its centre: a cone.
// V = pi r^2 h / 3 = pi 100 30 / 3 = 1000 pi = 3141.593 mm^3.
return loft([
  { z: 0, outline: [[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }] },
  { z: 30, point: [0, 0] },
]);

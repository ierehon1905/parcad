// An L outline lofted (ruled) to the same L at half size, 20 mm up. Every
// section of a ruled loft between homothetic outlines is the outline scaled
// linearly, so the solid is a frustum of the L: V = h/3 (A1 + A2 + sqrt(A1 A2))
// = 20/3 (450 + 112.5 + 225) = 5250 mm^3. A re-entrant section pairs as
// literally as a convex one; a pairing that slipped a vertex would twist the
// walls and read short.
const l = [[0, 0], [30, 0], [30, 10], [10, 10], [10, 25], [0, 25]];
return loft([
  { z: 0, outline: l },
  { z: 20, outline: l.map(([x, y]) => [x / 2, y / 2]) },
]);

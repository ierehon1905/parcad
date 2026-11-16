// A triangular prism laid on its side by two quarter turns and clipped to
// 20 mm by a box, the way a windscreen is cut from an extruded outline.
//
// The closed form. The triangle (-10, -10) (10, -10) (0, 10) is 200 mm² and
// 40 long. The turn about y lays its length along x, and the turn about x
// then stands the triangle up in y and z: base on z = -10 from y = -10 to 10,
// apex at (y, z) = (0, 10). The box keeps x = -10..10, so 200·20 = 4000 mm³
// in a 20 mm cube's box, with five faces: the prism's base at (0, 0, -10),
// its two slopes at (0, ±5, 0), and the box's two faces across it at
// (±10, 0, -10 + 20/3).
const glass = extrude([[-10, -10], [10, -10], [0, 10]], 40).tag("glass");
return glass.rotate("y", 90).rotate("x", 90).intersect(box(20, 60, 60));

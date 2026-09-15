// A parabolic segment: the quadratic Bezier from (-10, 0) to (10, 0) with its
// control point at (0, 20) is the parabola y = 10 - x^2 / 10, closed by its
// chord. Archimedes' quadrature: the segment is 2/3 of the 20 x 10 box around
// it, 400/3 mm^2, so V = 3 * 400 / 3 = 400 mm^3 exactly.
return extrude([[-10, 0], [10, 0], { bezier: [[0, 20]] }], 3);

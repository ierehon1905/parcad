// The loft primitive against a closed form, and the honesty split that came
// with it: the B-rep builds this exactly, the implicit backend must refuse by
// name rather than approximate — a quietly wrong field would let a probe
// confidently measure a part that does not exist.
//
// Ruled walls between a 40 mm and a 20 mm square, 30 mm apart, make a
// prismatoid: V = h/6 * (A_bottom + 4*A_mid + A_top)
//           = 30/6 * (1600 + 4*900 + 400) = 28000 mm^3.
// Each wall is a trapezoid with slant height sqrt(30^2 + 10^2):
//   A = 4 * (40 + 20)/2 * 31.6228 + 1600 + 400 = 5794.73 mm^2.
return loft([
  { z: 0, outline: [[-20, -20], [20, -20], [20, 20], [-20, 20]] },
  { z: 30, outline: [[-10, -10], [10, -10], [10, 10], [-10, 10]] },
]).tag("hopper");

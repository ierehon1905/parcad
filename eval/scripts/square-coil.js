// An authored section wound into a coil: a 2 mm square wire on a radius-10,
// pitch-5 helix, 2.75 turns, left-handed.
//
// The Frenet frame of a helix is its screw motion, so the square keeps its
// attitude to the axis all the way up, and with its centre on the spine the
// volume is the section's area times the helix's length:
//   L = 2.75 * sqrt((2 pi 10)^2 + 5^2) = 173.33383 mm
//   V = 4 * L                          = 693.33531 mm^3
// The exact B-rep reads 693.3357 through BRepGProp: four B-spline walls and
// two flat ends. Which way it winds is pinned by left-hand-hook.
return sweep(
  [[-1, -1], [1, -1], [1, 1], [-1, 1]],
  { helix: { radius: 10, pitch: 5, turns: 2.75, hand: "left" } },
).tag("coil");

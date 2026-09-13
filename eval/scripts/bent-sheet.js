// A bent sheet: a 200 x 3 strip swept along a floor run, a wall, and a
// slope, bent at 6 mm. The other classic laptop stand.
//
// The strip is 200 wide and 3 thick, and the bends turn it about its width,
// so only the 1.5 mm it reaches toward the inside of each bend is in the
// way of a 6 mm radius. The old check used the profile's full 100 mm reach
// and refused every sheet-metal part ever drawn.
//
// Pappus, with the profile symmetric about the path: V = 600 * centreline.
//   corner 1: 90 deg, tangent 6;  corner 2: 50.19 deg, tangent 2.81
//   legs  (100-6) + (60-6-2.81) + (78.10-2.81) = 220.48
//   arcs  6*pi/2 + 6*0.8760            = 14.68
//   V = 600 * 235.16 = 141 096 mm^3
return sweep(
  [[-100, -1.5], [100, -1.5], [100, 1.5], [-100, 1.5]],
  [[0, 0, 0], [100, 0, 0], [100, 0, 60], [40, 0, 110]],
  { bend: 6 },
).tag("sheet");

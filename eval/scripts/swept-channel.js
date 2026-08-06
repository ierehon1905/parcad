// The sweep primitive against a closed form: a 10 mm square bar routed
// around one 90-degree bend, which is pipe()'s path model with an authored
// section in place of the circle.
//
// The legs are 60 and 40 mm; a 20 mm centreline bend trims 20 mm off each,
// leaving 40 + 20 = 60 mm of straight run. The bend itself is Pappus:
// the centroid rides the 20 mm arc, so V = 100 * 20 * pi/2 = 3141.59.
//   V = 100 * 60 + 3141.59 = 9141.59 mm^3
//
// The implicit backend refuses this by name — an authored section along a
// bent path has no exact distance field — which is the same honesty split
// the loft cases pin. A *round* section stays a pipe(), exact in both.
return sweep(
  [[-5, -5], [5, -5], [5, 5], [-5, 5]],
  [[0, 0, 0], [60, 0, 0], [60, 40, 0]],
  { bend: 20 },
).tag("channel");

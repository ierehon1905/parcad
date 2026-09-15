// A 10 x 4 slot section — a 6 x 4 rectangle between arc centres at x = +-3,
// capped by two half circles of radius 2 — swept along two 40 mm legs joined
// by a 20 mm bend. A = 24 + 4 pi = 36.566 mm^2, centroid on the path.
// Runs: 2 * (40 - 20) * A; bend: Pappus A * 20 * pi / 2.
// V = 40 A + 10 pi A = 2611.421 mm^3.
return sweep(
  [[-3, -2], [3, -2], { through: [5, 0] }, [3, 2], [-3, 2], { through: [-5, 0] }],
  [[0, 0, 0], [40, 0, 0], [40, 40, 0]],
  { bend: 20 },
);

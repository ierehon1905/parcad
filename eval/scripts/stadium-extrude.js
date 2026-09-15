// A slot outline: a 20 x 10 rectangle with a half circle on each end, one
// drawn with { through }, the other with { radius }. Area 200 + 25 pi, so
// V = 20 (200 + 25 pi) = 4000 + 500 pi = 5570.796 mm^3, and the two end walls
// are exact cylinders.
return extrude(
  [[-10, -5], [10, -5], { through: [15, 0] }, [10, 5], [-10, 5], { radius: 5 }],
  20,
);

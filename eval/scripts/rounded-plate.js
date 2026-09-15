// A 40 x 20 plate with four 3 mm corner radii, as { at, round } corners.
// Area 800 - 4 * (9 - 9 pi / 4) = 800 - 9 (4 - pi), so
// V = 5 * (800 - 9 (4 - pi)) = 3961.372 mm^3.
const r = 3;
return extrude(
  [
    { at: [-20, -10], round: r },
    { at: [20, -10], round: r },
    { at: [20, 10], round: r },
    { at: [-20, 10], round: r },
  ],
  5,
);

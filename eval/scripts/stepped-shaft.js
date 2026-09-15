// A stepped shaft as one re-entrant revolve section rather than a union of
// two convex ones: r = 10 for 10 mm, then r = 6 for 20 mm.
// V = pi (10^2 * 10 + 6^2 * 20) = 1720 pi = 5403.539 mm^3.
return revolve([
  [0, 0],
  [10, 0],
  [10, 10],
  [6, 10],
  [6, 30],
  [0, 30],
]);

// A bend tighter than the profile's own reach: the 10 mm square section
// extends 7.07 mm from the path, so a 5 mm centreline bend would drive the
// inner side of the section through itself. OCCT resolves that into a
// self-intersecting surface rather than an error, so the graph refuses it
// first — in `Op::validate_sweep`, which the bounds share, so the refusal
// carries the same words and the same numbers wherever it is read.
return sweep(
  [[-5, -5], [5, -5], [5, 5], [-5, 5]],
  [[0, 0, 0], [60, 0, 0], [60, 40, 0]],
  { bend: 5 },
);

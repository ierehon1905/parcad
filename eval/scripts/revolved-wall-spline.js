// A turned bell, a spline wall from a 20 mm base to a 5 mm neck, 45 tall,
// hollowed to a 1 mm wall open at both ends: what `wall` is for. Its inset is
// the exact offset of the revolved spline, measured by the kernel as it is
// built and read back here as a 1 mm wall everywhere.
return revolve(
  [[0, 0], [20, 0], [20, 10], { spline: [[12, 20], [6, 35]], start: [-1, 0.6], end: [0, 1] }, [5, 45], [0, 45]],
  { wall: 1 },
).tag("bell");

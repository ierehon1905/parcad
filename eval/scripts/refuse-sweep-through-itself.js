// A square bar routed round three bends whose last leg runs straight back
// across the first. The path has no bend tighter than the bar, so every rule
// the graph holds a path to passes, and OpenCASCADE sweeps it into a solid
// its own validity check accepts: the two legs simply occupy the same space,
// counted twice in the volume. The sweep now refuses it by where the path
// comes back to itself, confirmed by the kernel's self-intersection check.
return sweep(
  [[-3, -3], [3, -3], [3, 3], [-3, 3]],
  [[0, 0, 0], [40, 0, 0], [40, 30, 0], [20, 30, 0], [20, -20, 0]],
  { bend: 5 },
);

// A loft between two outlines with an arc each, the second listed from its far
// corner so the walls pass through the axis. Curved sections are not proven by
// the graph; the kernel's self-intersection check runs on the built loft and
// finds the walls meeting at mid-height, which its validity check passed.
return loft([
  { z: 0, outline: [[-10, -10], [10, -10], { through: [14, 0] }, [10, 10], [-10, 10]] },
  { z: 20, outline: [[10, 10], [-10, 10], { through: [-14, 0] }, [-10, -10], [10, -10]] },
]);

// A ruled loft from a square to the same square listed from its opposite
// corner: every wall pairs a corner with the one across from it, so all four
// walls pass through the axis at mid-height and the solid is two pyramids
// touching at a point. OpenCASCADE builds it and its validity check passes it.
// The graph refuses it exactly, naming the height where the walls meet.
const square = [[-10, -10], [10, -10], [10, 10], [-10, 10]];
return loft([
  { z: 0, outline: square },
  { z: 20, outline: [square[2], square[3], square[0], square[1]] },
]);

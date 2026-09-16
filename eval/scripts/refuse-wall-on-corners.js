// A wall asked of sections drawn as corners. The kernel makes a wall by
// stepping sampled points inward and fitting both skins on one set of knots,
// so it takes only { fit } sections and must say so, naming how to get one.
return loft(
  [
    { z: 0, outline: [[-20, -20], [20, -20], [20, 20], [-20, 20]] },
    { z: 30, outline: [[-10, -10], [10, -10], [10, 10], [-10, 10]] },
  ],
  { wall: 2 },
);

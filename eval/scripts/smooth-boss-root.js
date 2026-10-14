// The concave case: a boss standing on a plate, its root eased into the floor
// and its rim into the wall. Both edges are closed circles, so each blend is
// one closed stripe with no corner. At the root the blend must match zero
// curvature on the floor and, across the wall's generators, zero again on the
// cylinder; at the rim the same pair the other way up. A fillet here steps by
// 1/2 per mm at every rail; smooth() is measured to step by none.
const plate = box(40, 40, 4).at(0, 0, 2).tag("plate");
const boss = cylinder(6, 8).at(0, 0, 8).tag("boss");
return union(plate, boss)
  .edges({ between: ["plate", "boss"] })
  .expect({ count: 1 })
  .smooth(2)
  .edges({ on: "boss", curve: "circle", at: { z: "max" } })
  .expect({ count: 1 })
  .smooth(2);

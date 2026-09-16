// A 20 x 20 x 2 plate with a Ø2 hole through it at the origin and a Ø6 recess
// 1 mm deep from below, its axis 4.25 mm along X: between the two circles the
// plate is 4.25 - 1 - 3 = 0.25 mm thick, over the recess's 1 mm of height. The
// control box's base plate shipped this at 0.457, between a screw hole and a
// foot recess.
const plate = box(20, 20, 2).at(0, 0, 1).tag("plate");
const hole = cylinder(1, 4).at(0, 0, 1).tag("hole");
const recess = cylinder(3, 2).at(4.25, 0, 0).tag("recess");
return plate.cut(hole, recess);

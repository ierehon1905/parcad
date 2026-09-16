// A Ø20 tube whose Ø16 bore is set 1 mm off its axis, toward +X. The wall is
// 3 mm on the -X side and 1 mm on the +X side, where the two circles are
// nearest: 10 - (8 + 1).
const tube = cylinder(10, 30).tag("tube");
const bore = cylinder(8, 40).at(1, 0, 0).tag("bore");
return tube.cut(bore);

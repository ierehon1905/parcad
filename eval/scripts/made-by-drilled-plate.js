// A 40 x 30 x 10 plate drilled through at x = 8 and its four upright edges
// rounded by 4.
//
// The closed form. The rounded outline is 1200 - (4 - π)·16 = 1186.265 mm²,
// the Ø6 hole takes π·9 = 28.274, and 10 mm of that is 11579.912 mm³.
// Eleven faces: top, bottom, four sides, four rounds and the hole's wall.
//
// Where the faces the case points at are. The hole's wall is a whole
// cylinder, centred on its axis at (8, 0, 0). A round is a quarter cylinder
// about (16, 11); its centroid is 4·2√2/π = 3.601 from that axis along the
// diagonal, at (18.546, 13.546, 0). The bottom is the outline less the hole,
// so its centroid is pushed -8·28.274/1157.991 = -0.195 along x.
const hole = cylinder(3, 20).at(8, 0, 0);
return box(40, 30, 10).cut(hole).edges("|Z").fillet(4);

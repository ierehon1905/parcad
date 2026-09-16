// The cavity is sealed, and no tool of the cut opens it: the bore stops 1 mm
// above the cavity's ceiling and the corner hole is elsewhere. Judged on the
// result, the refusal must still fire, and say where the void is and which
// tool made it.
const outside = box(40, 40, 30);
const cavity = box(30, 30, 24).tag("cavity");
const bore = cylinder(4, 2).at(0, 0, 14);
const hole = cylinder(2, 40).at(17, 17, 0);
return outside.cut(cavity, bore, hole);

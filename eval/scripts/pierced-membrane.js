// A 20 x 20 x 5 plate with a Ø4 blind hole from below that stops 0.004 mm
// short of the top face: a membrane 0.004 thick, which is an ordinary blind
// hole by every topological measure (docs/GOTCHAS.md, "The cut that seals a
// void is refused").
const plate = box(20, 20, 5).at(0, 0, 2.5).tag("plate");
const depth = 5 - 0.004;
const hole = cylinder(2, depth + 1).at(0, 0, (depth - 1) / 2).tag("hole");
return plate.cut(hole);

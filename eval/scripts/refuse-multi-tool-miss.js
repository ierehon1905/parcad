// Two holes that cut and one drawn 100 mm off the part. The refusal names the
// tool that misses, and only that one: the other two are clear of blame even
// though one of them is a duplicate that removes nothing new.
const hole = cylinder(3, 20).at(-10, 0, 0);
const same = cylinder(3, 20).at(-10, 0, 0);
const stray = cylinder(3, 20).at(100, 0, 0);
return box(40, 40, 10).cut(hole, same, stray);

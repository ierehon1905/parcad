// A Ø7 boss with a Ø3 hole through it whose axis is 2√2 from the boss's. The
// hole breaks out of the side, and where the two circles cross the material
// between them closes to nothing at the angle their normals make there:
// acos((3.5² + 1.5² - 8) / (2 · 3.5 · 1.5)) = acos(0.61905) = 51.75°. The
// control box's grille cut its screw boss this way.
const boss = cylinder(3.5, 10).tag("boss");
const hole = cylinder(1.5, 20).at(2, 2, 0).tag("hole");
return boss.cut(hole);

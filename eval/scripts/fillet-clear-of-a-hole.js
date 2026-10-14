const base = box(40, 30, 5).at(0, 0, 2.5).tag("base");
const fin = box(3, 30, 15).at(0, 0, 12.5).tag("fin");
const hole = cylinder(2.5, 20).at(8.0, 0, 0);
return union(base, fin).cut(hole)
  .edges({ between: ["base", "fin"], dihedral: "concave" })
  .fillet(2);

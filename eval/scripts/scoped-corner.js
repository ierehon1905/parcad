// A plate with a boss on it, both named. The seam is asked for by the two
// names it lies between; the boss's top rim by the boss's name and the
// corner it makes, the blend's own tangent lines being smooth; and after
// the whole thing turns, the plate's upright edges are still the plate's.
// Every one of those selectors survives the union, the fillets and the
// rotation that would once have lost the names.
const plate = box(60, 40, 10).tag("plate");
const boss = cylinder(10, 20).at(0, 0, 10).tag("boss");   // 5 mm buried, 15 proud
return union(plate, boss)
  .edges({ between: ["plate", "boss"] })
  .expect({ count: 1 })
  .fillet(2)
  .edges({ on: "boss", dihedral: "convex" })
  .expect({ count: 1 })
  .fillet(1)
  .rotate("z", 30)
  .edges({ on: "plate", parallel: "z" })
  .expect({ count: 4 })
  .fillet(3);

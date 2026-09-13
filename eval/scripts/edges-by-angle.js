// An L bracket, its edges chosen by the corner they make rather than by
// where they sit. The inside corner where the upright meets the base is the
// one concave edge along Y; after it is filled, the convex edges along Y
// are the base's two bottom edges, its free top edge, and the upright's two
// top edges — five, the fillet's own tangent lines not among them, because
// they are smooth and the query says convex.
const base = box(60, 40, 10).at(0, 0, 5);
const upright = box(10, 40, 40).at(-25, 0, 20);
return union(base, upright)
  .tag("l")
  .edges({ dihedral: "concave", parallel: "y" })
  .expect({ count: 1 })
  .fillet(4)
  .edges({ dihedral: "convex", parallel: "y", longerThan: 20 })
  .expect({ count: 5 })
  .fillet(2);

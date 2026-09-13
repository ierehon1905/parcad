// Two blocks side by side, the low one named. Its top edges are asked for
// as the highest edges *of the low block*, which sit 20 mm below the part's
// highest edges; the one along the tall block is an inside corner and the
// dihedral term leaves it, so three get the radius.
//
// The tall block is wider than the low one on purpose. Were their side faces
// coplanar, the unify pass would merge each pair into one face carrying
// both names — a face made from two features belongs to both — and the tall
// block's top edges, bordering that merged face, would be "on" the low block
// too. A feature boundary that a merge erases is not one a name can keep.
const low = box(40, 30, 10).at(20, 0, 5).tag("low");
const tall = box(40, 40, 30).at(-20, 0, 15).tag("tall");
return union(low, tall)
  .edges({ on: "low", at: { z: "max" }, dihedral: "convex" })
  .expect({ count: 3 })
  .fillet(2);

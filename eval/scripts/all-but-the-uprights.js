// The selector a session wrote as ">Z and not |Z" and could not say.
//
// The compact form is a conjunction of extrema and directions and has no
// negation; the query form now does. This breaks every outside edge of the
// block except the four upright ones, which is eight of its twelve: the
// finished block has 14 faces: its own six, plus one round per treated edge.
// docs/COIN_HOLDER_REVIEW.md, L2.
const block = box(40, 20, 10).tag("block");
return block
  .edges({ dihedral: "convex", not: { parallel: "z" } })
  .expect({ count: 8 })
  .fillet(1);

// An L outline: convex everywhere except the inside corner.
//
// A re-entrant section has no exact distance field, so the implicit and exact
// backends would disagree about where its surface is. Both must refuse it, and
// the refusal has to name the thing to build instead — a union of convex prisms
// is exactly how an L-plate is cut anyway.
return extrude(
  [
    [0, 0],
    [30, 0],
    [30, 10],
    [10, 10],
    [10, 25],
    [0, 25],
  ],
  6,
);

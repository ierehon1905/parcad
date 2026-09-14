// An L outline: convex everywhere except the inside corner.
//
// A re-entrant section is refused rather than built: on one the kernel's
// vertex pairing is a silent guess, and the refusal has to name the thing to
// build instead — a union of convex prisms is exactly how an L-plate is cut
// anyway.
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

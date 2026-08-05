// Offsetting a cuboid is lowered as grow-then-fillet-all-12-edges, which for a
// box is the Minkowski sum exactly rather than an approximation of it.
return box(30, 20, 10).offset(3);

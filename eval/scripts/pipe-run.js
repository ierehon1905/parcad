// A 10 mm hydraulic line with two right-angle joints, routed in 3D.
//
// The volume is a closed form, which is the point of routing it through square
// corners. It is not the obvious one, and getting it wrong is instructive: two
// perpendicular runs meeting at a corner *overlap*, in a quarter of the
// Steinmetz solid, and the ball that fills the corner is three quarters
// redundant. Per joint, with r = 5:
//
//   -4r^3/3 for the overlap the two runs share      = -166.667
//   +pi*r^3/3 for the quarter ball neither covers   = +130.900
//
// So 130 mm of run at r = 5 is pi*25*130 - 2*166.667 + 2*130.900 = 10138.64
// mm3, and a first pass that treats the runs as disjoint reads 3% high.
//
// That is what a polyline sweep *is*, and why it can be exact in both backends
// where a swept spline cannot: capsules, not a sweep solver.
return pipe(
  [
    [0, 0, 0],
    [60, 0, 0],
    [60, 40, 0],
    [60, 40, 30],
  ],
  10,
);

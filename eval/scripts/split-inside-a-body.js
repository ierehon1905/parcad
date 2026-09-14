// A named body that is accidentally in two pieces, beside one that is whole.
//
// `whole` is a 10 mm cube. `split` is meant to be one body and is the union
// of two 4 x 10 x 10 bars 2 mm apart, so it is 800 mm³ in two pieces: the
// defect `pieces` on that body reports as 2 while the part-level count says
// three free-standing pieces for a part that names two bodies. The gap from
// the cube's face at x = 5 to the nearer bar's face at x = 25 is 20 mm.
const whole = box(10, 10, 10).tag("whole");
const split = union(box(4, 10, 10).at(-3, 0, 0), box(4, 10, 10).at(3, 0, 0))
  .at(30, 0, 0)
  .tag("split");
return { whole, split };

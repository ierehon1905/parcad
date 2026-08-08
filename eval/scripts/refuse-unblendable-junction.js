// Two equal-radius cylinders crossing at 90° — docs/DSL_GAPS.md §4's third
// row. Their union's seam is two ellipses crossing at the tangency saddle, so
// four seam edges converge at one vertex and a 2 mm blend cannot solve the
// corner. The refusal must say so from the seam itself — branch count and
// location — and name a smaller radius measured to build (1.13 mm, verified as
// a fresh evaluation), instead of blaming the radius or killing the worker.
const run = cylinder(21, 100).rotate("y", 90).tag("run");
const branch = cylinder(21, 60).at(0, 0, 20).tag("branch");
return union(run, branch, { blend: 2 }).tag("body");

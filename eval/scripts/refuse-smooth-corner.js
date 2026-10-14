// All twelve edges of a block smoothed: three meet at each corner, and the
// kernel's corner patch for a curvature-continuous blend is not even tangent
// (2.8 degrees off, measured), so the build is refused before the kernel is
// asked, naming a corner and the fix.
return box(40, 30, 10).edges({ dihedral: "convex" }).expect({ count: 12 }).smooth(2);

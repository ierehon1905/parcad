// The simplest curvature-continuous blend: a block's four vertical edges
// eased into its sides. Each blend runs face to face between two planes and
// ends where the top and bottom cut it, so there is no corner to solve; the
// section is a quintic that leaves each side with zero curvature, where a
// fillet(4) would leave it with 1/4 per mm. The reply's curvature_jump_per_mm
// is that step measured on the built solid along every rail.
return box(40, 30, 10).edges("|Z").expect({ count: 4 }).smooth(4);

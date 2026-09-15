// An M10 bolt and a printed nut, 0.25 mm clearance each, in phase: the
// section that drew a dark line down the bolt's axis.
//
// The rod is centred, z -15 to 15, its core at the basic minor diameter
// 8.376 less twice the clearance: radius 3.938. The nut is z 1 to 9, and its
// hole reaches radius 5.25 (the major 10, plus the clearance), so on the
// y = 0 plane the nut is solid from |x| = 5.25 out to 8.5.
const bolt = threadedRod("M10", 30, { clearance: 0.25 });
const nut = box(17, 17, 8).at(0, 0, 9 - 4)
  .cut(threadedHole("M10", 8, { through: true, clearance: 0.25 }).at(0, 0, 9));
return { bolt, nut };

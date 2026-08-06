// An O-ring groove turned into the wall of a boss: the torus's main job.
//
// This was a known defect and is now a regression guard. The cut itself was
// always sound — OCCT performed it — and then `UnifySameDomain`, which every
// result passes through to weld away imprint edges, segfaulted on the two
// coaxial circular seams it left behind. A box took the same cut without
// complaint, and so did a cylinder cut by a torus large enough to enter from
// the side; it was specifically the coaxial groove that died. OCCT 8.0.1 fixed
// it, the marker came off, and the assertions stayed exactly as they were.
return cylinder(12, 14).cut(torus(11.5, 1)).tag("glanded");

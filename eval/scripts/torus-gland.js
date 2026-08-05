// An O-ring gland turned into the face of a boss: the torus's main job.
//
// KNOWN DEFECT, and the case exists to hold the ground. The cut itself is
// sound — OCCT performs it — and then `UnifySameDomain`, which every result
// passes through to weld away imprint edges, segfaults on the two coaxial
// circular seams it leaves behind. A box takes the same cut without complaint,
// and so does a cylinder cut by a torus large enough to enter from the side;
// it is specifically the coaxial groove that dies, which is the shape every
// O-ring gland is.
return cylinder(12, 14).cut(torus(11.5, 1)).tag("glanded");

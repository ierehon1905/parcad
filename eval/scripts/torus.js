// A ring of 30 mm major radius and 4 mm minor.
//
// The fourth closed form in the corpus: a torus is 2*pi^2*R*r^2 = 9474.820 mm3
// and 4*pi^2*R*r = 4737.410 mm2. Both are checked, because the two backends
// build it by unrelated means — OCCT revolves a circular edge, the implicit
// field is hypot(hypot(x,y) - R, z) - r — and a shape whose volume is right can
// still have the wrong surface.
return torus(30, 4);

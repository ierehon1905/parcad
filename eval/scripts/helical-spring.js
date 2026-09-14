// A helical pipe against a closed form: a 2 mm wire wound into a spring of
// radius 10, pitch 5, three turns.
//
// The section is a circle perpendicular to the helix with its centre on it, so
// the tube's volume is its section times the helix's length (the tube formula;
// the curvature term vanishes with the centroid on the spine):
//   L = 3 * sqrt((2 pi 10)^2 + 5^2) = 189.09145 mm
//   V = pi * 1^2 * L                = 594.04831 mm^3
//   A = 2 pi * 1 * L + 2 pi * 1^2   = 1194.37980 mm^2
// The exact B-rep reads 594.0481 mm^3 and 1194.3795 mm^2 through BRepGProp;
// the volume recorded below is the mesh's, which a 1 mm tube under-reads by
// half a percent at 0.01 mm deflection.
return pipe({ helix: { radius: 10, pitch: 5, turns: 3 } }, 2).tag("spring");

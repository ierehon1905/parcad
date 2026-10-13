// Sixteen beads on a sphere, every rim rounded. The big sphere's seam ends on
// the +X bead's rim in two kernel edges, and `.expect` counts it as one, so the
// count a reply reports must be 16 too, or pasting it fails the build.
return sphere(8)
  .union(repeat(sphere(1), polar(16, 7.4)))
  .edges({ dihedral: "concave" })
  .expect({ count: 16 })
  .fillet(0.2);

// Sixteen beads on a sphere, every rim rounded by 0.15. Each round is one
// B-spline face closed by a seam, and the kernel's check of a face against
// itself used to read that seam as a crossing: at 0.15 on arm64, at 0.2 on
// x86_64 Windows (docs/GOTCHAS.md, "A closed fillet ring was reported as
// crossing itself at its seam"). The part is sound at every radius tried.
return sphere(8)
  .union(repeat(sphere(1), polar(16, 7.4)))
  .edges({ dihedral: "concave" })
  .expect({ count: 16 })
  .fillet(0.15);

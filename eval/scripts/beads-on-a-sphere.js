// Sixteen small balls fused onto a big one. Where a bead's cap straddles its
// own seam the intersection circle comes back as two arcs, and merging them
// used to leave the new edge's pcurves one arc behind its 3D circle; the bead
// collapsed to its pole. Patch 0004, docs/GOTCHAS.md.
return sphere(8).union(repeat(sphere(1), polar(16, 7.4)));

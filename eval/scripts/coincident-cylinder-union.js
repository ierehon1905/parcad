// A cylinder unioned with itself turned 45 degrees about its axis. The fuse
// splits the side into two faces on one surface; the seam-pcurve pre-pass then
// dropped a representation both pieces were using, which left a B-rep of
// 523.60 mm³ instead of 1570.80 and a mesh that did not close.
// docs/GOTCHAS.md has the table.
return union(cylinder(5, 20), cylinder(5, 20).rotate("z", 45));

// A ball unioned with itself turned a quarter about X — what a figurine does
// whenever a mirrored or rotated copy of a head, joint or eye lands on the
// original. The fuse is exact (4188.79 mm³, one face); the copy's seam used to
// stay on that face as an internal wire, and the mesher tessellated a closed
// 728 mm³ fragment of it. Found building a unicorn over MCP on 2026-09-14;
// docs/GOTCHAS.md has the table.
return union(sphere(10), sphere(10).rotate("x", 90));

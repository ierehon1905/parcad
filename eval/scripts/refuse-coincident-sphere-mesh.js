// A ball unioned with itself turned a quarter about X — what a figurine does
// whenever a mirrored or rotated copy of a head, joint or eye lands on the
// original. The fuse is exact (4188.79 mm³, one face), but that face
// tessellates as a closed 728 mm³ fragment: watertight, one body, and most of
// the ball missing from the preview and the STL. Found building a unicorn over
// MCP on 2026-09-14; docs/GOTCHAS.md has the table.
return union(sphere(10), sphere(10).rotate("x", 90));

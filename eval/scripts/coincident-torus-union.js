// A torus unioned with itself turned 30 degrees about its axis. UnifySameDomain
// welds the two halves back into one face of the whole surface with no boundary
// at all, which the mesher skipped: no triangles, "a mesh with no vertices".
// That face is now rebuilt with its natural bounds. docs/GOTCHAS.md has the table.
return union(torus(10, 2), torus(10, 2).rotate("z", 30));

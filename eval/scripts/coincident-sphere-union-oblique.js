// The same ball turned 37 degrees instead of 90: the copy's seam crosses the
// face at an angle, and the fragment the mesher made of it did not even close
// (4 open mesh edges). Fixed by the same internal-edge drop as
// coincident-sphere-union; docs/GOTCHAS.md has the table.
return union(sphere(10), sphere(10).rotate("x", 37));

// The head-to-head retainer trial that came out wrong: the query was meant for
// the concave seam where the cylinder meets the block's top, and a circle next
// to a +z face is also the cylinder's convex top rim. The reply said only
// `edges: 2`, and the model reported the part done.
const block = box(59.49, 118.381, 13.314).at(29.745, 59.1905, 6.657);
const cyl = cylinder(29.745, 36.664).at(29.745, 118.381, 18.332);
const fused = block.union(cyl);
return fused.fillet(2, { curve: "circle", adjacentTo: { faceNormal: "+z" } });

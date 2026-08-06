// The same tangency as tangent-blend.js, at the size it was found on: the
// stock of reference/retainer.js, a disc standing flush on the end of a plate
// exactly as wide as the disc. Here the spine is a single half-circle whose
// two ends both land on tangencies, one of them on the disc's own seam — the
// single-stripe form of the pinch, which additionally needs the tangent
// generator between the old vertex and the pinch apex as a new edge.
//
// Before the vendored patch 0001-tangent-pinch-corner this returned a solid
// with 22 open edges and IsDone() == true, and the wreckage stayed inside the
// part's bounding box, so only BRepCheck_Analyzer saw it — this was the case
// proving the validity gate pulls its weight. It now holds the measured
// result down instead; the full retainer built on this stock measures
// 143829.66 mm3 against the Fusion 360 reference B-rep's 143825.6.
const W = 59.49;
const plate = box(W, 118.381, 13.314).at(W / 2, 118.381 / 2, 13.314 / 2);
const disc = cylinder(W / 2, 36.664).at(W / 2, 118.381, 36.664 / 2);
return union(plate, disc, { blend: 2 }).tag("stock");

// The same tangency as refuse-tangent-blend.js, at the size it was found on.
//
// This case exists because it defeats the cheaper of the two post-conditions.
// The 20 mm version breaches its bounding box by 0.23 mm and containment
// refuses it; here the wreckage stays inside the part, so only
// BRepCheck_Analyzer sees the unorientable faces and the self-intersecting
// wire. Delete the validity gate and this goes green while accepting a solid
// with 22 open edges.
const W = 59.49;
const plate = box(W, 118.381, 13.314).at(W / 2, 118.381 / 2, 13.314 / 2);
const disc = cylinder(W / 2, 36.664).at(W / 2, 118.381, 36.664 / 2);
return union(plate, disc, { blend: 2 }).tag("stock");

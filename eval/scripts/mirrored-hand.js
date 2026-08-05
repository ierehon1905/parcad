// A chiral shape and its reflection, joined on the mirror plane.
//
// The peg is off-centre in y and stands up in z, so the part has a handedness:
// reflecting it in x moves the peg to -x and leaves y and z alone, while the
// half turn about x that OCCT's only bound mirror actually performs would send
// the peg to -y and -z instead. Both join to a solid of the same volume, so
// volume alone cannot tell them apart — the bounding box can, and does: a
// reflection keeps this part 10 mm deep, while the half turn would hang the
// second peg below the plate and make it 20 mm.
const half = union(
  box(20, 10, 4).at(10, 0, 2),
  cylinder(2, 10).at(16, 3, 5),
);

return union(half, half.mirror("x"));

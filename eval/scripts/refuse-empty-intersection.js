// Two solids with nothing in common. The intersection is the fit check
// anyone writes — lay the real object in its pocket and ask what overlaps —
// and its right answer is an interference of zero, said as a number with
// both extents, not "the result has no faces" three stages later.
return intersect(box(20, 20, 20), box(20, 20, 20).at(50, 0, 0));

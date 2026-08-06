// A boss standing on a plate exactly as wide as the boss, so the boss wall is
// tangent to both side walls. Nothing here is oversized: the radius is small,
// the material is thick, and the plain union is valid and watertight.
//
// A blend has to end somewhere, and here it ends against a face it touches
// without crossing. The fillet's width falls to zero at the tangency, so the
// correct answer is a torus face whose boundary touches itself at a point —
// degenerate, not merely awkward. OCCT returns an unorientable B-spline patch
// and a self-intersecting wire with IsDone() == true.
//
// Measured on this shape: the plain union is 14279.05 mm3, the correct blend
// adds 43.78 mm3 of fillet, this one adds 5.17. Missing 88% of the fillet.
const plate = box(20, 40, 10).at(10, 20, 5);
const boss = cylinder(10, 25).at(10, 20, 17.5);
return union(plate, boss, { blend: 2 });

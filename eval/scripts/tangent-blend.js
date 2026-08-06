// A boss standing on a plate exactly as wide as the boss, so the boss wall is
// tangent to both side walls. Nothing here is oversized: the radius is small,
// the material is thick, and the plain union is valid and watertight.
//
// A blend has to end somewhere, and here it ends against a face it touches
// without crossing. The fillet's width falls to zero at the tangency, so the
// correct answer is a torus face that pinches to a point where its inner
// contact circle touches the side wall. Stock OpenCASCADE built a corner patch
// there instead — unorientable, self-intersecting, 0.23 mm outside the part,
// missing 88% of the fillet — with IsDone() == true; the vendored patch
// 0001-tangent-pinch-corner builds the pinch exactly, and this case holds the
// measured result down. The plain union is 14279.05 mm3; the blend adds
// 47.50 mm3 of fillet, slightly more than the ~43.8 the rail-blend answer at
// 1e-4 mm clearance adds, because the trimmed torus keeps material the
// pivoting ball sheds.
const plate = box(20, 40, 10).at(10, 20, 5);
const boss = cylinder(10, 25).at(10, 20, 17.5);
return union(plate, boss, { blend: 2 });

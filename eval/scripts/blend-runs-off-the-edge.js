// The control for refuse-tangent-blend.js, and the reason that one is a
// refusal rather than a known defect.
//
// Same plate, same boss, same radius, half a millimetre of clearance. The
// fillet still runs clean off the plate on both sides — the rolling ball's
// centre passes outside the material and the torus reaches 1.5 mm past the
// side wall, so OCCT has to trim the strip against two planes and close it.
// It does. Running off the end of a face is therefore not what the tangent
// case gets wrong, which is worth its own case because it is the obvious
// suspect and it is innocent.
const plate = box(21, 40, 10).at(10.5, 20, 5);
const boss = cylinder(10, 25).at(10.5, 20, 17.5);
return union(plate, boss, { blend: 2 });

// The control for tangent-blend.js, from the era when that one was a
// refusal: it proved the trigger was tangency, not overrun.
//
// Same plate, same boss, same radius, half a millimetre of clearance. The
// fillet still runs clean off the plate on both sides — the rolling ball's
// centre passes outside the material and the torus reaches 1.5 mm past the
// side wall, so OCCT has to trim the strip against two planes and close it.
// It does, and always did. Running off the end of a face was therefore
// never what the tangent case got wrong — worth its own case because
// overrun is the obvious suspect and it is innocent, and because this
// rail-and-pivot trimming is a different construction from the pinched
// torus the tangent case now builds.
const plate = box(21, 40, 10).at(10.5, 20, 5);
const boss = cylinder(10, 25).at(10.5, 20, 17.5);
return union(plate, boss, { blend: 2 });

// A 20 mm square inset by 2 and extruded 5: (20 - 4)^2 * 5 = 1280 mm3 exactly,
// six planar faces. The inset is the kernel's offset of the outline, measured
// before it is used; the closed form is what that measurement is held to.
const square = [[-10, -10], [10, -10], [10, 10], [-10, 10]];
return extrude(inset(square, 2), 5).tag("core");

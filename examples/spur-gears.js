// A spur gear pair, module 2, 20 and 30 teeth, meshed at their centre
// distance: a 20 mm pitch-radius pinion on a 5 mm motor shaft driving a 30 mm
// pitch-radius wheel on an 8 mm shaft, 1.5 : 1.
//
// Every flank is the involute of its base circle, drawn by spurGearOutline as
// a curve the script certifies against the formula; the report's
// curve_bound_mm is how far that proof lets a flank stray. Tip and root are
// exact arcs; below the base circle the flank runs straight in to the
// root, where a hobbed gear would have a trochoid that nothing touches.
//
// Each tooth is thinned by 0.1 mm at the pitch circle. Meshed centred, that
// play is shared between the two flanks of every tooth space, and involute
// flanks in mesh stay the same distance apart along the line of action, so
// the two bodies come no closer than 0.1 · cos 20° = 0.094 mm anywhere —
// which `between_bodies` measures on the built solids.
const m = 2;
const pinionTeeth = 20;
const wheelTeeth = 30;
const width = 8;
const backlash = 0.1;
const centres = (m * (pinionTeeth + wheelTeeth)) / 2;

const pinion = extrude(spurGearOutline({ module: m, teeth: pinionTeeth, backlash }), width)
  .cut(cylinder(5 / 2, width * 2))
  .tag("pinion");

// A tooth of the pinion points along +X, at the wheel. The wheel's teeth also
// start on +X, so turning it half a tooth puts a space on -X to take it.
const wheel = extrude(spurGearOutline({ module: m, teeth: wheelTeeth, backlash }), width)
  .cut(cylinder(8 / 2, width * 2))
  .rotate("z", 180 / wheelTeeth)
  .at(centres, 0, 0)
  .tag("wheel");

return { pinion, wheel };

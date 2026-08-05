// A knurled control knob for a 6 mm shaft with a flat — the D-bore that stops
// the knob from spinning on the shaft.
//
// The knurl is 24 axial flutes cut with a small cylinder each. That is what a
// moulded knob really has; a machined diamond knurl is a different process and
// would need a helical cut, which this DSL cannot express (docs/DSL_GAPS.md).

const knobDia = 30;
const knobH = 16;
const shaft = 6;
const flatDepth = 0.5;   // how much of the shaft is flatted (a "D" shaft)
const flutes = 24;
const fluteDia = 3;

const body = cylinder(knobDia / 2, knobH).tag("body");

// One flute, placed on the rim once and spun around the axis. Centred on the
// rim, so half of it cuts in and half cuts air — which is what makes the flute
// a scallop rather than a slot.
const flute = cylinder(fluteDia / 2, knobH * 1.2).at(knobDia / 2, 0);
const knurl = around(flute, flutes).tag("knurl");

// The D-bore: a round hole with one side flatted off. The flat is what
// transmits torque, so its depth is a fit dimension, not decoration.
//
// Intersection, not union. Adding a box to the cutter would push the flat out
// past the bore and cut a keyway slot into the knob instead — the shape is
// "the cylinder, trimmed" and has to be written that way.
const bore = intersect(
  cylinder(shaft / 2, knobH * 2),
  box(shaft, shaft - flatDepth, knobH * 2).at(0, -flatDepth / 2),
).tag("d_bore");

// A dished top, so a thumb sits in the knob rather than on it. A sphere large
// enough that only its bottom cap enters the material makes a shallow dish;
// its radius sets how shallow.
const dish = sphere(26).at(0, 0, knobH / 2 + 26 - 2).tag("dish");

const turned = body.cut(knurl, dish).tag("turned");

// The bore is cut in its own tagged step so the lead-in below can name it.
// Rolled into the same cut as the knurl, `generatedBy` would also cover the
// forty-odd flute edges around the bottom rim, and the count would say nothing.
const bored = turned.cut(bore).tag("bored");

// Break the bottom of the bore — the edge that is pushed onto the shaft.
//
// Five edges: a D-bore's outline is the straight flat plus an arc, and the
// kernel returns that arc in pieces. `role: "hole"` would match nothing here,
// because it wants a closed circle and this outline is not one. The count is
// still the assertion — it is measured, and a change to it means the bore
// stopped being a D.
return bored
  .edges({ generatedBy: "bored", at: { z: "min" } })
  .expect({ count: 5 })
  .chamfer(0.5)
  .tag("bore_lead_in");

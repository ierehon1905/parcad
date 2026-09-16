// A tube of radius 10 thickened 2 mm three ways: centred on it, all outward
// (its normal side) and all inward.
const tube = surfaceExtrude([[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }], 30, { closed: true });
return {
  both: tube.thicken(2),
  out: tube.thicken(2, { side: "out" }).at(30, 0, 0),
  in: tube.thicken(2, { side: "in" }).at(60, 0, 0),
};

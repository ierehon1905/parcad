// The tube trimmed by the vertical plane x = y drawn as a surface, keeping
// what lies behind it: half the tube.
const tube = surfaceExtrude([[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }], 30, { closed: true });
return tube.trim(surfaceExtrude([[-20, -20], [20, 20]], 40), { keep: "back" });

// The tube cut by the plane z = 5, keeping what is above: 10 of its 30 mm.
const tube = surfaceExtrude([[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }], 30, { closed: true });
return tube.trim({ plane: { point: [0, 0, 5], normal: [0, 0, 1] } }, { keep: "above" });

// A rugged case for a 100 x 40 x 26 object on a print-in-place hinge, drawn
// open and flat: the base, the lid folded 180 degrees away from it, the pin
// of the lid through the base's knuckles with a 0.4 mm gap, and a bead on
// the lid's latch that rides over the base's groove wall as it shuts. The
// lid folds about +x through (0, AY, AZ); negative angles close it.
brief({ envelope: [110, 110, 25], printer: "generic FDM" });
const W = 107;
const D = 47;
const AY = -28;
const AZ = 16.2;
const LY = -56;
const wallCut = box(102, 42, 15);
let base = box(W, D, 16).at(0, 0, 8).cut(wallCut.at(0, 0, 9.5));
base = base.cut(box(20, 1.3, 2).at(0, 23.35, 10.5).tag("groove"));
const sx = [-43, -21.5, 0, 21.5, 43];
for (const i of [0, 2, 4]) {
  base = base.union(box(21, 10, 16).at(sx[i], -27, 8), cylinder(4, 21).rotate("y", 90).at(sx[i], AY, AZ));
}
base = base.cut(cylinder(1.9, 110).rotate("y", 90).at(0, AY, AZ));
let lid = box(W, D, 16).at(0, LY, 8).cut(wallCut.at(0, LY, 9.5));
for (const i of [1, 3]) {
  lid = lid.union(box(21, 10, 16).at(sx[i], -29, 8), cylinder(4, 21).rotate("y", 90).at(sx[i], AY, AZ));
}
lid = lid.union(cylinder(1.5, 106).rotate("y", 90).at(0, AY, AZ));
lid = lid.union(box(18, 4, 16).at(0, -80, 8), box(18, 2, 8).at(0, -81, 19.5), box(16, 1.2, 1.6).at(0, -79.6, 21.9).tag("bead"));
const object = box(100, 40, 26).at(0, 0, 16.2).reference();
const object2 = box(100, 40, 26).at(0, LY, 16.2).reference();
const hinge = { body: "lid", about: "x", at: [0, AY, AZ] };
return {
  base,
  lid,
  object,
  object2,
  checks: [
    { clear: ["base", "object"], atLeast: 0.9 },
    { clear: ["lid", "object2"], atLeast: 0.9 },
    { clear: ["base", "lid"], atLeast: 0.3 },
    { turns: { ...hinge, degrees: [0, -170] }, clearOf: ["base"], atLeast: 0.3 },
    { turns: { ...hinge, degrees: [-170, -180] }, catches: "base", deeperThan: 0.2 },
    { turns: { ...hinge, degrees: [0, -170] }, clearOf: ["object"], atLeast: 0.3 },
  ],
};

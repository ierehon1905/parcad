// The hinged box with a tall tab added to the base's far wall: the lid clears
// the tab shut and open, and sweeps through it on the way.
const W = 107;
const D = 47;
const AY = -28;
const AZ = 16.2;
const LY = -56;
const wallCut = box(102, 42, 15);
let base = box(W, D, 16).at(0, 0, 8).cut(wallCut.at(0, 0, 9.5));
const sx = [-43, -21.5, 0, 21.5, 43];
for (const i of [0, 2, 4]) {
  base = base.union(box(21, 10, 16).at(sx[i], -27, 8), cylinder(4, 21).rotate("y", 90).at(sx[i], AY, AZ));
}
base = base.union(box(20, 2, 20).at(0, 22.5, 26).tag("tab"));
base = base.cut(cylinder(1.9, 110).rotate("y", 90).at(0, AY, AZ));
let lid = box(W, D, 16).at(0, LY, 8).cut(wallCut.at(0, LY, 9.5));
for (const i of [1, 3]) {
  lid = lid.union(box(21, 10, 16).at(sx[i], -29, 8), cylinder(4, 21).rotate("y", 90).at(sx[i], AY, AZ));
}
lid = lid.union(cylinder(1.5, 106).rotate("y", 90).at(0, AY, AZ));
return {
  base,
  lid,
  checks: [{ turns: { body: "lid", about: "x", at: [0, AY, AZ], degrees: [0, -170] }, clearOf: ["base"], atLeast: 0.3 }],
};

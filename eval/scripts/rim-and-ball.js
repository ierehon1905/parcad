// Two bodies that both read `touching`, and only one of them is resting on
// anything.
//
// The cup is a Ø60 tube with a 1 mm floor and a 3 mm wall, so its rim is a
// flat ring Ø54 inside and Ø60 out: π(30² − 27²) = 537.212 mm² for a lid to
// sit on. The lid is a flat Ø60 disc laid on it, and seats on the whole of
// it, in one patch. The ball rests on the floor inside and touches it at a
// single point: the same verdict, 0 mm² of contact, 0 patches. A reader with
// only the verdict cannot tell the two apart — docs/COIN_HOLDER_REVIEW.md
// §2.3, where one repeated point at the +x end of a part was read as a
// seated stack.
const cup = cylinder(30, 12).at(0, 0, 6).cut(cylinder(27, 12).at(0, 0, 7)).tag("cup");
const lid = cylinder(30, 3).at(0, 0, 13.5).tag("lid");
const ball = sphere(5).at(0, 0, 6).tag("ball");
return {
  cup,
  lid,
  ball,
  checks: [{ touching: ["cup", "lid"], contactAtLeast: 500, why: "the lid seats on the rim, not on its edge" }],
};

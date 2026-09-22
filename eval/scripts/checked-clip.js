// A part whose checks all hold: a latch, the catch that bites it, and the
// foot the latch rests on. The catch reaches REACH = 1.8 mm into the latch,
// so the two share 1.8 x 10 x 4 = 72 mm³ against a check asking for 30; the
// foot's top face lies exactly on the latch's underside, `touching`; and
// nothing is thinner than the 2 mm foot. Editing REACH to 0.4 leaves 16 mm³,
// which fails the first check with that number: eval/field/is-the-catch-
// still-caught.md asks that question.
const REACH = 1.8;
const latch = box(20, 10, 4).tag("latch");
const catchArm = box(6, 10, 4).at(10 + 3 - REACH, 0, 0).tag("catch");
const foot = box(20, 10, 2).at(0, 0, -3).tag("foot");
return {
  latch,
  catch: catchArm,
  foot,
  checks: [
    { interferes: ["catch", "latch"], atLeast: 30, why: "the catch must bite the latch" },
    { touching: ["latch", "foot"] },
    { wall: { min: 1 } },
  ],
};

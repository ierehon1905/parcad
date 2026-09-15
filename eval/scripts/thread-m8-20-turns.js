// Twenty turns of M8 × 1.25, right- and left-handed. Same closed form as
// thread-m8-3-turns: V = π r1² L + 2π L / P · 1.325052 with r1 = 3.323418,
// which at L = 25 is 1033.991431 mm³ for either hand.
const right = threadedRod("M8", 20 * 1.25).tag("right");
const left = threadedRod("M8", 20 * 1.25, { hand: "left" }).at(20, 0, 0).tag("left");
return { right, left };

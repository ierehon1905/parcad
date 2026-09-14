// A part printed in two halves: a bored block split at its centreline with a
// 0.5 mm kerf, the right half being the mirror of the left.
//
// The closed form. The one-piece block is 60 x 40 x 20 minus a Ø10 bore:
// 48000 - π·25·20 = 46429.204 mm³. Each half is a 29.75 x 40 x 20 box
// (23800 mm³) less the part of the bore beyond x = 0.25 from its axis — a
// circular segment of area r²·acos(d/r) - d·√(r² - d²) = 36.77095 mm² at
// r = 5, d = 0.25, times the 20 mm height = 735.419 mm³ — so each half is
// 23064.581 mm³ and the two sum to 46129.162: the block minus the kerf's
// 300.042 mm³ of material. The halves face each other across planes at
// x = ±0.25, so their clearance is 0.5 mm exactly.
const gap = 0.5;
const block = box(60, 40, 20).cut(cylinder(5, 30));
const halfW = 30 - gap / 2;
const left = block.intersect(box(halfW, 50, 30).at(-(gap / 2 + halfW / 2), 0, 0)).tag("left");
const right = left.mirror("x").tag("right");
return { left, right };

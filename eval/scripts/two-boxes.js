// Two boxes that stay two: the smallest part in several bodies.
//
// 10 x 10 x 10 at the origin and 20 x 10 x 10 centred at x = 30, so the gap
// between them is 30 - 5 - 10 = 15 mm exactly, and the volumes are 1000 and
// 2000 mm³ by inspection. Nothing is fused: the part is 3000 mm³ in two
// pieces, and that is the intent rather than a defect.
const left = box(10, 10, 10).tag("left");
const right = box(20, 10, 10).at(30, 0, 0).tag("right");
return { left, right };

// A turned cup, radius 10, 20 tall, 1 mm wall, open at the bottom: the cavity
// is the same section stepped in by 1, drawn at radius -1 on the axis so the
// inset's axis edge lands on it, and 2 past the bottom so the floor of the
// inset lies outside the part. pi * (100 - 81) * 20 + pi * 81 * 1 =
// 461 pi = 1448.274 mm3, and a 1 mm wall measured.
const outside = revolve([[0, 0], [10, 0], [10, 20], [0, 20]]).tag("cup");
const cavity = revolve(inset([[-1, -2], [10, -2], [10, 20], [-1, 20]], 1));
return outside.cut(cavity);

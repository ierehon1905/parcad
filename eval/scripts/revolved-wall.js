// A turned cup, radius 10, 20 tall, with a 1 mm wall and a floor: the section
// hollowed by `wall`, its top open. pi * (100 - 81) * 20 + pi * 81 * 1 = 461 pi
// = 1448.274 mm3, and a 1 mm wall measured. The cavity is the section stepped
// in by 1, drawn to radius -1 on the axis and 2 past the open top, which
// `wall` does so the script does not have to.
return revolve([[0, 0], [10, 0], [10, 20], [0, 20]], { wall: { thickness: 1, bottom: "closed" } }).tag("cup");

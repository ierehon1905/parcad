// A turned sleeve, radius 10, 20 tall, whose bottom is a shallow cone from
// [0, 2] to [10, 0], hollowed by a 1 mm wall open at that cone and closed at the
// top: the open end is a sloped straight edge, so the cavity's floor is that
// edge moved 2 out along its normal and met with the wall's line. What is
// left is the 9..10 ring under the lid, less the cone under it: 461 pi -
// 2 pi * (10^2 - 9^2 - (10^3 - 9^3) / 15) = 1448.274 - 5.864 = 1442.410 mm3.
return revolve([[0, 2], [10, 0], [10, 20], [0, 20]], { wall: { thickness: 1, top: "closed" } }).tag("sleeve");

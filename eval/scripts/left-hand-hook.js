// Which way a helix winds, pinned by where half a turn of it goes. Starting at
// +X, a right-handed helix turns anticlockwise seen from above as it rises and
// spends its half turn at y >= 0; this left-handed one spends it at y <= 0,
// reaching y = -11 and stopping 0.08 mm past y = 0, where its end discs,
// perpendicular to the rising wire, lean over.
//
// Volume cannot tell the two apart and the recorded size is an extent, not a
// position, so a 1 mm marker ball stands clear of both at y = +20: with this
// hand the part is 32 mm deep in Y, with the other it would be 21.08.
//   V = pi * 1^2 * 0.5 * sqrt((2 pi 10)^2 + 5^2) + 4/3 pi = 103.19684 mm^3
return union(
  pipe({ helix: { radius: 10, pitch: 5, turns: 0.5, hand: "left" } }, 2),
  sphere(1).at(0, 20, 0),
).tag("hook");

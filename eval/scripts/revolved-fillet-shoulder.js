// A 20 mm r = 10 cylinder whose top outer corner is rounded by r = 3 in the
// section: a radius authored in section, not a rolling-ball fillet.
//
// Pappus: the removed spandrel (a 3 x 3 square minus a quarter disc) has area
// s = 9 - 9 pi / 4 = 1.931417 and its centroid sits d = 3 (5/6 - pi/4) / (1 - pi/4)
// = 0.670104 in from the corner, at radius 10 - d = 9.329896. So
// V = pi 100 20 - 2 pi 9.329896 s = 6283.185 - 113.222 = 6169.963 mm^3.
return revolve([
  [0, 0],
  [10, 0],
  { at: [10, 20], round: 3 },
  [0, 20],
]);

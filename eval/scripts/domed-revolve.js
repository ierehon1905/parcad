// A cylinder with a hemispherical cap, drawn as one section whose top is a
// quarter arc from the rim to the axis. V = pi 10^2 20 + (2/3) pi 10^3
// = 2000 pi + 2000 pi / 3 = 8377.580 mm^3; the cap is an exact sphere.
const r = 10;
return revolve([
  [0, 0],
  [r, 0],
  [r, 20],
  { through: [r * Math.SQRT1_2, 20 + r * Math.SQRT1_2] },
  [0, 20 + r],
]);

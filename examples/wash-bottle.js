// A wash bottle: a turned body whose outline is one section — straight
// walls, a spline shoulder, a neck with a rolled bead and a radiused base —
// hollowed by a second section, with a smooth spout tube curving out of the
// shoulder.
//
// Everything curved is drawn in section rather than filleted afterwards:
// `{ at, round }` for the base radius, `{ spline }` for the shoulder,
// `{ through }` for the bead on the neck. The spout is `pipe({ spline })`,
// one exact curve rather than runs and bends.

const bodyR = 32;
const shoulderZ = 70;
const neckR = 11;
const neckZ = 114;
const topZ = 132;
const wall = 2;

const outer = revolve([
  [0, 0],
  { at: [bodyR, 0], round: 6 },
  [bodyR, shoulderZ],
  { spline: [[28, 95], [16, 108]], start: [0, 1], end: [-0.4, 1] },
  [neckR, neckZ],
  [neckR, topZ - 6],
  { through: [neckR + 2, topZ - 4] },
  [neckR, topZ - 2],
  [neckR, topZ],
  [0, topZ],
]).tag("body");

// The cavity: the same outline brought in by the wall, open through the neck.
const cavity = revolve([
  [0, wall],
  { at: [bodyR - wall, wall], round: 4 },
  [bodyR - wall, shoulderZ],
  { spline: [[26, 94], [14, 106]], start: [0, 1], end: [-0.4, 1] },
  [neckR - wall, neckZ],
  [neckR - wall, topZ + 1],
  [0, topZ + 1],
]).tag("cavity");

const spout = pipe({ spline: [[20, 0, 96], [40, 0, 112], [62, 0, 116], [80, 0, 108]] }, 5).tag("spout");
const bore = pipe({ spline: [[20, 0, 96], [40, 0, 112], [62, 0, 116], [80, 0, 108]] }, 2.6);

return union(outer, spout).cut(cavity, bore);

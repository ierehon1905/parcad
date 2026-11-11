// A four-shelf pantry rack with 24 jars on it, the jars as reference bodies:
// 31 bodies, 465 pairs in `between_bodies`. Most jars lean a few degrees
// toward the back and sit 1 mm up on a pad; one of them, jar3e, reaches
// 4.034 mm into shelf4, which only the measured pair says: its height, 208,
// is under the 214 mm between the shelves, and the trigonometry on its lean
// gives 4.33, where its rim tops out, not the depth the kernel measures. The
// reply-budget test and eval/field/which-jar-hits-the-shelf.md build it.
const W = 568;
const D = 180;
const T = 16;
const H = 722;
const TOPS = [16, 246, 476, 706];

const side = (x) => box(T, D, H).at(x, 0, H / 2);
const bodies = {
  left: side(-(W + T) / 2),
  right: side((W + T) / 2),
  back: box(W + 2 * T, 6, H).at(0, D / 2 + 3, H / 2),
};
TOPS.forEach((top, i) => {
  bodies[`shelf${i + 1}`] = box(W, D, T).at(0, 0, top - T / 2);
});

const RADII = [32, 36, 40, 34, 38, 30];
const HEIGHTS = [
  [150, 182, 196, 160, 175, 188],
  [170, 155, 208, 186, 168, 181],
  [176, 162, 190, 185, 208, 172],
  [120, 140, 110, 130, 150, 125],
];
const LEAN = [
  [0, 4, 0, 7, 0, 3],
  [5, 0, 2, 8, 0, 6],
  [3, 6, 0, 4, 9, 0],
  [0, 5, 0, 8, 0, 4],
];

TOPS.forEach((top, s) => {
  let x = -W / 2 + 12;
  RADII.forEach((r, j) => {
    const h = HEIGHTS[s][j];
    const lean = LEAN[s][j];
    x += r;
    let jar = cylinder(r, h).at(x, -10, top + h / 2);
    if (lean > 0) {
      const a = (lean * Math.PI) / 180;
      const lowest = (h / 2) * Math.cos(a) + r * Math.sin(a);
      jar = cylinder(r, h).rotate("x", lean).at(x, -10, top + 1 + lowest);
    }
    bodies[`jar${s + 1}${"abcdef"[j]}`] = jar.reference();
    x += r + 14;
  });
});

return bodies;

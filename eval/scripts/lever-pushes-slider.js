// A lever on a pin drops 18° and pushes a slider along x, a card riding on
// the slider. The slider's law follows the lever's lower corner exactly, so
// the two stay 0.1 mm apart the whole stroke.
const PIN = [0, 0, 40];
const base = box(90, 30, 4).at(0, 0, 2);
const lever = box(6, 10, 30).at(0, 0, 25).union(cylinder(4, 12).rotate("x", 90).at(0, 0, 40));
const slider = box(20, 10, 8).at(13.1, 0, 14);
const card = box(16, 2, 14).at(13.1, 0, 25);
const push = (t) => {
  const a = (18 * t * Math.PI) / 180;
  return 30 * Math.sin(a) + 3 * (Math.cos(a) - 1);
};
return {
  base,
  lever,
  slider,
  card,
  mechanism: {
    lever: { turns: { about: "y", at: PIN, degrees: [0, -18] } },
    slider: { slides: { along: "x", mm: push } },
    card: { on: "slider" },
  },
  checks: [
    { moves: "all", atLeast: 0.3 },
    { pushes: ["lever", "slider"], within: 0.15 },
  ],
};

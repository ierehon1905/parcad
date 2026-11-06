// The lever and slider of lever-pushes-slider, the slider's law missing the
// corner's own drop: 30·sin a instead of 30·sin a + 3(cos a − 1). The gap
// opens to 0.1 + 3(1 − cos 18°) = 0.247 mm at the end of the stroke.
const PIN = [0, 0, 40];
const base = box(90, 30, 4).at(0, 0, 2);
const lever = box(6, 10, 30).at(0, 0, 25);
const slider = box(20, 10, 8).at(13.1, 0, 14);
const push = (t) => 30 * Math.sin((18 * t * Math.PI) / 180);
return {
  base,
  lever,
  slider,
  mechanism: {
    lever: { turns: { about: "y", at: PIN, degrees: [0, -18] } },
    slider: { slides: { along: "x", mm: push } },
  },
  checks: [
    { moves: "all", atLeast: 0.3 },
    { pushes: ["lever", "slider"], within: 0.15 },
  ],
};

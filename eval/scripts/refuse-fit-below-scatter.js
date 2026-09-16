// A circle with 0.3 mm of noise on every point, fitted at 0.01 mm: below the
// scatter, the fit can only interpolate, and the curve it makes loops between
// the points. Refused naming the tolerance to raise, never built.
let s = 7;
const rand = () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296);
const n = 120;
const points = Array.from({ length: n }, (_, i) => {
  const a = (2 * Math.PI * i) / n;
  const r = 20 + 0.3 * (rand() - 0.5);
  return [r * Math.cos(a), r * Math.sin(a)];
});
return extrude([{ fit: points, tolerance: 0.01 }], 10);

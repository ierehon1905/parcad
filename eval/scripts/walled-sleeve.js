// A ruled sleeve: a circle of radius 20 lofted through three heights as a
// 2 mm wall, open at both ends. The wall is vertical, so the step is the
// wall itself: pi * (20^2 - 18^2) * 40 = 9550.442 mm3.
const n = 120;
function ring(r) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [r * Math.cos(a), r * Math.sin(a)];
  });
}
const sections = [0, 15, 40].map((z) => ({ z, outline: [{ fit: ring(20), tolerance: 0.001 }] }));
return loft(sections, { wall: 2 }).tag("sleeve");

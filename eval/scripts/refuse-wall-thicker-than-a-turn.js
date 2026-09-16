// A six-lobed outline whose valleys turn tighter than the 5 mm wall asked of
// it: stepped inward, the inside folds there. The kernel must refuse rather
// than return a wall that is not 5 mm, and name the wall as the thing to thin.
const n = 180;
function lobes(r) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * i) / n;
    const rr = r * (1 + 0.3 * Math.cos(6 * a));
    return [rr * Math.cos(a), rr * Math.sin(a)];
  });
}
const sections = [0, 20, 40].map((z) => ({ z, outline: [{ fit: lobes(20), tolerance: 0.01 }] }));
return loft(sections, { smooth: true, wall: 5 });

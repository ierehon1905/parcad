// A quarter of a cylinder of radius 20, lofted through the same fitted arc at
// two heights.
const arc = Array.from({ length: 16 }, (_, i) => {
  const a = (Math.PI / 2) * (i / 15);
  return [20 * Math.cos(a), 20 * Math.sin(a)];
});
return surfaceLoft([
  { z: 0, curve: [{ fit: arc, tolerance: 0.001 }] },
  { z: 30, curve: [{ fit: arc, tolerance: 0.001 }] },
]);

// A five-pointed star lofted to itself turned three corners on. The walls
// between them cross each other part way up, and the kernel built the loft
// with a negative volume, -3382 mm3, which every later boolean would have read
// as all of space minus a star. Its validity check passed it; the mesh closed.
// The graph finds the first height where a corner of the outline lands on an
// edge, from quadratics in the height rather than sampled slices.
const star = Array.from({ length: 10 }, (_, i) => {
  const a = (Math.PI * i) / 5;
  const r = i % 2 ? 6 : 20;
  return [r * Math.cos(a), r * Math.sin(a)];
});
return loft([
  { z: 0, outline: star },
  { z: 20, outline: star.map((_, i) => star[(i + 3) % star.length]) },
]);

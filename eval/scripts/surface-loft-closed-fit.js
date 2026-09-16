// A tube of radius 15 lofted smooth through fitted circles at three heights.
const ring = Array.from({ length: 48 }, (_, i) => {
  const a = (2 * Math.PI * i) / 48;
  return [15 * Math.cos(a), 15 * Math.sin(a)];
});
return surfaceLoft([0, 20, 40].map((z) => ({ z, curve: [{ fit: ring, tolerance: 0.001 }] })), { closed: true, smooth: true });

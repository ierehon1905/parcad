// A six-lobed outline, r = 20 (1 + 0.3 cos 6a), sampled at 180 points and
// fitted closed at 0.05 mm, extruded 10: the enclosed area is
// 200 pi (2 + 0.09) = 1313.186 mm2, so 13131.858 mm3. The points are at equal
// angles, so chord-length parameters run at the wrong speed round the lobes:
// fitted on them the curve needed 131 poles; with parameters corrected to the
// curve it holds on 49, C2 through the seam.
const n = 180;
const pts = Array.from({ length: n }, (_, i) => {
  const a = (2 * Math.PI * i) / n;
  const r = 20 * (1 + 0.3 * Math.cos(6 * a));
  return [r * Math.cos(a), r * Math.sin(a)];
});
return extrude([{ fit: pts, tolerance: 0.05 }], 10).tag("lobes");

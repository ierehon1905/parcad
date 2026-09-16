// One curve of 24 pleats 4 mm deep round a 60 mm circle, lofted as a surface
// through three heights and thickened 1.4 mm about it. Every section is the
// same curve, so the surface is a cylinder over it with no Gaussian curvature,
// and the wall — centred, closed along its normals — holds exactly
// t × L × h: 1.4 × 556.7461 × 200 = 155888.9 mm³, L being the curve's length,
// half the surface's free edge length (1113.492 mm). OCCT's whole-face volume
// integral reads 556182 mm³ here; the reported volume is the mesh's.
const n = 480;
const curve = Array.from({ length: n }, (_, i) => {
  const a = (2 * Math.PI * i) / n;
  const r = 60 + 4 * Math.cos(24 * a);
  return [r * Math.cos(a), r * Math.sin(a)];
});
const sections = [0, 100, 200].map((z) => ({ z, curve: [{ fit: curve, tolerance: 0.05 }] }));
return surfaceLoft(sections, { closed: true, smooth: true }).tag("pleat").thicken(1.4);

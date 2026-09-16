// A smooth loft through five fitted circles, radius 30 at z = 0 to 10 at
// z = 40: the kernel fits every section on one knot vector and interpolates
// the poles across them, which reproduces the straight generator exactly, so
// the solid is the frustum pi * 40 / 3 * (30^2 + 30 * 10 + 10^2) = 54454.273.
const n = 120;
function ring(r) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [r * Math.cos(a), r * Math.sin(a)];
  });
}
const sections = [0, 10, 20, 30, 40].map((z) => ({ z, outline: [{ fit: ring(30 - z / 2), tolerance: 0.001 }] }));
return loft(sections, { smooth: true }).tag("frustum");

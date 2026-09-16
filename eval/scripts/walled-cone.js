// A cone frustum lofted as a 2 mm wall with a closed floor. The sections are
// circles sampled at 120 points and fitted at 0.001 mm; radius 30 at z = 0 to
// 10 at z = 40, so the wall leans at slope 1/2 and the kernel steps each
// section in by 2 * sqrt(1.25) = 2.236 mm to make the wall 2 mm square to
// the surface. The cavity is the frustum of radii 30 - 1 - 2.236 at z = 2
// (the floor) and 10 - 2.236 at z = 40.
const n = 120;
function ring(r) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [r * Math.cos(a), r * Math.sin(a)];
  });
}
const sections = [0, 10, 20, 30, 40].map((z) => ({ z, outline: [{ fit: ring(30 - z / 2), tolerance: 0.001 }] }));
return loft(sections, { smooth: true, wall: { thickness: 2, bottom: "closed" } }).tag("cone");

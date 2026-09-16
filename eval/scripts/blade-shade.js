// Four open blades of a shade, each lofted through fitted arcs, thickened to
// 1.4 mm and hung from one ring.
const blades = 4;
const levels = 5;
const height = 60;
const radius = (t) => 30 + 6 * Math.sin(Math.PI * t);
function blade(i) {
  return surfaceLoft(
    Array.from({ length: levels }, (_, k) => {
      const t = k / (levels - 1);
      const centre = ((360 / blades) * i + 20 * t) * Math.PI / 180;
      const points = Array.from({ length: 12 }, (_, j) => {
        const s = j / 11;
        const a = centre - 0.6 + 1.2 * s;
        const r = radius(t) + 4 * s;
        return [r * Math.cos(a), r * Math.sin(a)];
      });
      return { z: t * height, curve: [{ fit: points, tolerance: 0.01 }] };
    }),
    { smooth: true },
  ).tag("blade");
}
const ring = revolve([[27, height - 3], [38, height - 3], [38, height + 1], [27, height + 1]]).tag("ring");
return union(ring, ...Array.from({ length: blades }, (_, i) => blade(i).thicken(1.4)));

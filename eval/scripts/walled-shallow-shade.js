// A shade that lies nearly flat: a cone from radius 70 at z = 0 to radius 20
// at z = 10, slope 5, so its wall leans 11.3 degrees off horizontal, walled
// 1 mm and open at both ends. Square to the surface, 1 mm is a sideways step
// of sqrt(26) = 5.099 mm, so the cavity is the frustum of radii 70 - sqrt(26)
// and 20 - sqrt(26) over the same height.
const n = 120;
function ring(r) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [r * Math.cos(a), r * Math.sin(a)];
  });
}
const sections = [0, 5, 10].map((z) => ({ z, outline: [{ fit: ring(70 - 5 * z), tolerance: 0.001 }] }));
return loft(sections, { smooth: true, wall: 1 }).tag("shade");

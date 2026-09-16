// A spherical bowl of radius 40, its pole at the origin, walled 2 mm with a
// closed floor at z = 0.5: there the surface lies 9 degrees off horizontal,
// where a sideways step would have to be 12.8 mm wide. The sections are
// circles at equal steps of polar angle, fitted at 0.001 mm. The inside is
// the sphere of radius 38 about the same centre, cut by the floor at z = 2.5.
const n = 120;
const R = 40;
function ring(r) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [r * Math.cos(a), r * Math.sin(a)];
  });
}
const first = Math.acos((R - 0.5) / R);
const count = 40;
const sections = Array.from({ length: count }, (_, k) => {
  const theta = first + ((Math.PI / 2 - first) * k) / (count - 1);
  return { z: R - R * Math.cos(theta), outline: [{ fit: ring(R * Math.sin(theta)), tolerance: 0.001 }] };
});
return loft(sections, { smooth: true, wall: { thickness: 2, bottom: "closed" } }).tag("bowl");

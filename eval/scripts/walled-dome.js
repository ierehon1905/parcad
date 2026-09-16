// The upper half of a sphere of radius 40, from its equator at z = 0 to a
// closed cap at z = 39.5, where the surface lies 9 degrees off horizontal,
// walled 2 mm. The sections are circles at equal steps of polar angle,
// fitted at 0.001 mm. The inside is the sphere of radius 38, capped by the
// floor at z = 37.5.
const n = 120;
const R = 40;
function ring(r) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [r * Math.cos(a), r * Math.sin(a)];
  });
}
const last = Math.acos(39.5 / R);
const count = 40;
const sections = Array.from({ length: count }, (_, k) => {
  const phi = Math.PI / 2 - ((Math.PI / 2 - last) * k) / (count - 1);
  return { z: R * Math.cos(phi), outline: [{ fit: ring(R * Math.sin(phi)), tolerance: 0.001 }] };
});
return loft(sections, { smooth: true, wall: { thickness: 2, top: "closed" } }).tag("dome");

// A ruled loft through three fitted circles, r = 10, 20, 10 at z = 0, 10, 20.
// The smooth loft through them is the revolution of the parabola
// r(z) = 10 + 2z - z^2/10 through the three radii; the ruled one is the
// revolution of its two chords. Their facet sag is the furthest a chord is
// from the parabola in the profile plane, measured either way: the
// parabola's point at z = 5, r = 17.5, is 2.5 mm out from the chord
// r = 10 + z, which has slope 1, so 2.5 / sqrt(2) = 1.7678 mm square to it,
// and no point of either is further from the other.
const n = 240;
function ring(r) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * i) / n;
    return [r * Math.cos(a), r * Math.sin(a)];
  });
}
const sections = [
  { z: 0, outline: [{ fit: ring(10), tolerance: 0.001 }] },
  { z: 10, outline: [{ fit: ring(20), tolerance: 0.001 }] },
  { z: 20, outline: [{ fit: ring(10), tolerance: 0.001 }] },
];
return loft(sections, { smooth: false }).tag("barrel");

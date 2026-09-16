// A five-lobed star lofted to itself with its points listed eight further
// round, so each lobe's tip is paired with a point past the next valley. The
// sections are simple; the ruled walls between them are not: part way up, the
// outline at that height loops. A loft through fitted sections is skinned by
// parcad, and its crossings are found on the skin's own poles rather than by
// the kernel's self-intersection check, which took 16 s on a pleated shade;
// that check, run on this loft, finds the same crossing (face itself near
// (6.86, -20.78, 12.63)).
const n = 60;
function star(shift) {
  return Array.from({ length: n }, (_, i) => {
    const a = (2 * Math.PI * ((i + shift) % n)) / n;
    const r = 30 + 12 * Math.cos(5 * a);
    return [r * Math.cos(a), r * Math.sin(a)];
  });
}
return loft([
  { z: 0, outline: [{ fit: star(0), tolerance: 0.05 }] },
  { z: 25, outline: [{ fit: star(8), tolerance: 0.05 }] },
]);

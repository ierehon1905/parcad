// The five-lobed star of refuse-fitted-loft-crossing lofted to itself with its
// points listed seven further round: one short of the pairing whose walls
// cross, and clear of crossings (the kernel's self-intersection check agrees).
// The star r = 30 + 12 cos 5a reaches y = ±40.201 between its sampled points,
// where the samples reach only ±39.944, so the curve fitted through them is
// wider than its points by more than its tolerance — correctly. Two sections
// make the smooth loft the ruled one, so the facets sag nowhere.
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
  { z: 25, outline: [{ fit: star(7), tolerance: 0.05 }] },
]);

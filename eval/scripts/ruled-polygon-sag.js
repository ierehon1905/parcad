// Three squares growing linearly, half-widths 10, 15 and 20 at z = 0, 10, 20,
// lofted ruled. Their corners lie on straight lines, so the smooth loft
// through them is the same pyramid frustum and the facet sag is 0; the
// frustum is h/3 (A1 + A2 + sqrt(A1 A2)) = 20/3 (400 + 1600 + 800) =
// 18666.667 mm^3. This is the ThruSections path's measurement: a sag that
// read more than the mesh's own chord would be comparing the wrong faces.
const square = (a) => [[-a, -a], [a, -a], [a, a], [-a, a]];
return loft([
  { z: 0, outline: square(10) },
  { z: 10, outline: square(15) },
  { z: 20, outline: square(20) },
]).tag("frustum");

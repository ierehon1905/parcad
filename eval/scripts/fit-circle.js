// A circle of radius 20 sampled at 200 points and fitted closed, held within
// 0.01 mm of every sample, extruded 10: the analytic disc is
// pi * 400 * 10 = 12566.371 mm3. What the case proves: the fit builds as one
// closed curve (3 faces: two planes and one wall), its measured deviation is
// read back from the built part and sits under the tolerance, and the volume
// is within what a 0.01 mm tolerance round a 125.66 mm perimeter can move
// (12.6 mm3) of the closed form.
const n = 200;
const points = Array.from({ length: n }, (_, i) => {
  const a = (2 * Math.PI * i) / n;
  return [20 * Math.cos(a), 20 * Math.sin(a)];
});
return extrude([{ fit: points, tolerance: 0.01 }], 10).tag("disc");

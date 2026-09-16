// A fitted circle of radius 20 minus the same outline inset by 2, 10 tall: a
// ring of pi * (400 - 324) * 10 = 2387.610 mm3. The inset of a one-curve
// outline is one curve, offset by the kernel and measured to lie 2 mm inside
// it; the wall this leaves is what measure_wall_thickness reads as 2.
const n = 200;
const points = Array.from({ length: n }, (_, i) => {
  const a = (2 * Math.PI * i) / n;
  return [20 * Math.cos(a), 20 * Math.sin(a)];
});
const outline = [{ fit: points, tolerance: 0.01 }];
return extrude(outline, 10).cut(extrude(inset(outline, 2), 12)).tag("ring");

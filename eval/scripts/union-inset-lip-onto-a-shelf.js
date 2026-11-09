// One shelf of the plant-shelf model: an 18 mm base, and on it an 18 mm lip
// ring whose inner contour is the outline inset 18, both cut back to y = 36.
// The two touch only at z = 18, so the union's volume is the sum of the
// pieces, 4317514.76 + 441827.53 mm3. The base comes first on purpose: then
// the point classifier aims its ray at the base's underside, straight down,
// and that ray lies inside the lip's B-spline inner wall, which the union
// guard read as a point outside the union.
const sx = 960 / 930;
const S = (pts) => pts.map(([x, y]) => [x * sx, y > 215 ? 215 + (y - 215) * 0.75 : y]);
const pts = [[15, 0], [960, 0], ...S([[930, 180], [925, 250], [900, 320], [850, 375], [780, 405], [700, 410],
  [600, 385], [500, 340], [380, 285], [250, 245], [120, 228]]), [15, 215]];
const outline = [...pts.slice(0, 3), { spline: pts.slice(3, 13), start: [0, 1] }, ...pts.slice(13)];
const behindBack = box(3000, 72, 400);
const base = extrude(outline, 18).cut(behindBack).at(0, 0, 9);
const lip = extrude(outline, 18).cut(extrude(inset(outline, 18), 20)).cut(behindBack).at(0, 0, 27);
return base.union(lip);

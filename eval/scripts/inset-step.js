// A 100 x 60 rectangle whose right edge steps in by 3 at y = 40 - a door
// casing - inset by 10, deeper than the step, and extruded 5. The step's own
// edge is too short to leave an offset of its own at 10 mm, yet intersection
// joins keep the step: (10,10) (90,10) (90,30) (87,30) (87,50) (10,50),
// 80*40 - 3*20 = 3140 mm2, so 15700 mm3. The corner at (87,30) is 10*sqrt(2)
// from the outline's re-entrant corner at (97,40).
const outline = [[0, 0], [100, 0], [100, 40], [97, 40], [97, 60], [0, 60]];
return extrude(inset(outline, 10), 5);

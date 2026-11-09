// An L-shaped outline inset by 10 and extruded 5. Intersection joins keep the
// re-entrant corner a corner: the inset is the polygon (10,10) (90,10) (90,50)
// (50,50) (50,90) (10,90), 80*40 + 40*40 = 4800 mm2, so 24000 mm3. The corner
// at (50,50) is 10*sqrt(2) from the outline's corner at (60,60): the wall
// thickens into the valley, and the inset guard must read that as the
// corner it is rather than as slip.
const outline = [[0, 0], [100, 0], [100, 60], [60, 60], [60, 100], [0, 100]];
return extrude(inset(outline, 10), 5);

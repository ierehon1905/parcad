// A 100 x 60 rectangle with a 20 x 15 notch in its top edge, inset by 5 and
// extruded 5. The notch is wider than twice the inset, so it survives, 30
// wide and 15 deep: 90*50 - 30*15 = 4050 mm2, so 20250 mm3, with two
// re-entrant corners at (65,40) and (35,40), each 5*sqrt(2) from its outline
// corner.
const outline = [[0, 0], [100, 0], [100, 60], [60, 60], [60, 45], [40, 45], [40, 60], [0, 60]];
return extrude(inset(outline, 5), 5);

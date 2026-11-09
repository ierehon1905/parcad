// A 10 × 10 section standing 10 mm off the axis, inset by 2 and revolved: a
// ring of pi * (18² - 12²) * 6 = 3392.920 mm3. A revolve's section lies in XZ,
// which the inset's area guard once read as 0 mm² and refused as "grew".
const outline = [[10, 0], [20, 0], [20, 10], [10, 10]];
return revolve(inset(outline, 2)).tag("ring");

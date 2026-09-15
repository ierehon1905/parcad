// A pipe along a spline path whose points are collinear. The chord-length
// natural cubic through collinear points is the straight line itself, so this
// is a 30 mm cylinder of radius 3: V = 9 pi 30 = 848.230 mm^3. It holds the
// spline spine, the section's placement at its start and the sweep's frame to
// a number that has no bend in it to excuse a drift.
return pipe({ spline: [[0, 0, 0], [10, 0, 0], [30, 0, 0]] }, 6);

// A 20 x 5 bar with a 3 mm stem, inset by 2: the stem is narrower than twice
// the inset and vanishes, leaving the bar's inset, 16 x 1, and 80 mm3 at 5
// tall. The offset removed the loop the stem's sides fold into; its trace is
// a vertex at the stem's centre, so the 16 mm edge is two edges.
return extrude(inset([[-10, 0], [10, 0], [10, 5], [1.5, 5], [1.5, 20], [-1.5, 20], [-1.5, 5], [-10, 5]], 2), 5).tag("bar");

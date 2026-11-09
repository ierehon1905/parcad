// A 20 mm square with one corner rounded 0.5, inset by 1: the round is
// tighter than the inset, its offset vanishes and the inset has a corner
// there, an 18 mm square, 1620 mm3 at 5 tall.
return extrude(inset([[-10, -10], { at: [10, -10], round: 0.5 }, [10, 10], [-10, 10]], 1), 5).tag("core");

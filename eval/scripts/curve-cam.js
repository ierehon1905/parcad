// A closed cam r(t) = 20 + 3 cos t + sin 2t, drawn from the bare function, so
// its bound is estimated. Area (1/2)∫r² dt over a turn = π(400 + 9/2 + 1/2)
// = 405π, and 8 tall: 3240π = 10178.760 mm3.
const r = (t) => 20 + 3 * Math.cos(t) + Math.sin(2 * t);
return extrude([{ curve: (t) => [r(t) * Math.cos(t), r(t) * Math.sin(t)], from: 0, to: 2 * Math.PI, tolerance: 0.00005 }], 8).tag("cam");

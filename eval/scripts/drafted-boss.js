// A 40 mm square boss with 5° of draft on every wall, 20 mm tall.
//
// The third closed form in the corpus, after `cone.js` and `hex-prism.js`: a
// square frustum is h/3 * (A1 + A2 + sqrt(A1*A2)). The walls lean in by
// 20*tan(5°) = 1.750, so the top is 36.500 square and the volume is
// 29282.008 mm3.
//
// It is worth checking rather than assuming, because the two backends build a
// draft by unrelated means. OCCT lofts between the outline and its inset copy;
// the implicit field tilts each wall's half-plane out of vertical. If the inset
// and the tilt ever disagreed by a fraction of a degree, nothing would look
// wrong — the number here is what says so.
return extrude(
  [
    [-20, -20],
    [20, -20],
    [20, 20],
    [-20, 20],
  ],
  20,
  { draft: 5 },
);

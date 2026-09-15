// An L outline, re-entrant at its inside corner, extruded in one piece.
//
// A re-entrant section used to be refused: the implicit kernel had no exact
// distance field for one, and the reason recorded was "vertex pairing is a
// silent guess". A prism pairs nothing. Area 30*10 + 10*15 = 450 mm^2, so the
// solid is 450 * 6 = 2700 mm^3 exactly, 8 plane faces.
return extrude(
  [
    [0, 0],
    [30, 0],
    [30, 10],
    [10, 10],
    [10, 25],
    [0, 25],
  ],
  6,
);

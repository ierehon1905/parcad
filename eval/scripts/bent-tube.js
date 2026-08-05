// A 10 mm tube with a 15 mm centreline bend, the way tube is actually made.
//
// Closed form, and an easy one because the pieces meet on flat discs and never
// overlap: each straight run is pi*r^2*L, and a bend of radius R through angle
// d is pi*R*r^2*d. The runs are trimmed back to their tangent points — 15 mm
// each here, since a 90 degree bend of radius 15 needs R*tan(45) of straight —
// so 45 + 25 mm of run and a quarter turn gives
//
//   pi*25*70 + pi*15*25*(pi/2) = 5497.79 + 1850.55 = 7348.34 mm3
//
// This is the answer to "no sweep, then?". A swept spline has no exact field,
// but the two elements a tube is made of do, and this is both of them.
return pipe(
  [
    [0, 0, 0],
    [60, 0, 0],
    [60, 40, 0],
  ],
  10,
  { bend: 15 },
);

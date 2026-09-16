// A 4.76 mm blend on the seams of a plate, a thin fin and a boss standing
// 1 mm from it: the rounds on either side of that gap run into each other.
// The blend path's validity check passed the result and the mesh closed; the
// check on the faces the blend made finds them crossing.
return union(
  box(40, 30, 5.01),
  box(3.38, 16.07, 12).at(-0.59, -3.9, 9.12),
  cylinder(6.4, 12).at(-9.68, -1.94, 3.99),
  { blend: 4.76 },
);

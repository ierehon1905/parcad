// The request's numbers written as checks, on their own line instead of in
// the returned object. JavaScript reads `checks:` as a label, so the list is
// built and dropped, and the plate used to build with no checks and no word
// of it — a Haiku 5.5 field run wrote exactly this before `return plate;`.
const at = [[-30, -15], [30, -15], [-30, 15], [30, 15]];
let plate = box(80, 50, 6);
for (const [x, y] of at) plate = plate.cut(cylinder(3, 10).at(x, y, 0));
checks: [
  { size: { is: [80, 50, 6] } },
  { holes: { diameter: 6, through: true, at } },
];
return plate;

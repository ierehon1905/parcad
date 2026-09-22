// The coin holder's cups, and the overhang every printability number before
// print_check was blind to (docs/COIN_HOLDER_REVIEW.md, workflow §3 and §11).
//
// A 60 x 30 x 3 plate carries two Ø24 cups 14 mm tall, each with four
// retaining lips reaching 1.5 mm straight inward from the rim, 2 mm thick,
// their undersides 12 mm above the floor. Printed as drawn, +z up, every lip
// underside is a 0° ceiling with nothing beneath it: unsupported area is the
// lips' undersides, 8 × (the ring sector each lip is) = 8 × 12.0 mm², plus
// the arc faces. The part is watertight, one body, stands flat on its whole
// plate and fits every bed, and nothing in the source names an angle.
const plate = box(60, 30, 3).at(0, 0, 1.5).tag("plate");
const wall = 2;
const cupR = 12;
const cupH = 14;
const lipIn = 1.5;
const lipT = 2;
const cups = [-15, 15].map((x) => {
  const shell = cylinder(cupR + wall, cupH).at(x, 0, 3 + cupH / 2);
  const bore = cylinder(cupR, cupH + 1).at(x, 0, 3 + cupH / 2 + 0.5);
  return shell.cut(bore).tag("cup");
});
const lips = [-15, 15].flatMap((x) =>
  [0, 90, 180, 270].map((deg) =>
    box(lipIn + 0.5, 6, lipT)
      .at(cupR - lipIn / 2 + 0.25, 0, 0)
      .rotate("z", deg)
      .at(x, 0, 3 + cupH - lipT / 2)
      .tag("lip"),
  ),
);
return plate.union(...cups, ...lips);

// Three pegs placed on the plate's underside plane rather than its top, so
// each runs through the 6 mm plate and stands 2 mm out of the bottom — the
// plate-stand mistake, unblended so it builds. Everything the corpus usually
// checks is plausible here: the part is watertight, its height is a sensible
// sum, its volume is right for what was drawn. Only `stands_on` says what is
// wrong: three round patches of 50 mm² at z = −2 where a 2400 mm² slab at
// z = 0 was meant.
const plate = box(60, 40, 6).at(0, 0, 3).tag("plate");
const peg = cylinder(4, 32).at(0, 0, 14);
return union(plate, peg.at(-20, 0, 0), peg, peg.at(20, 0, 0)).tag("body");

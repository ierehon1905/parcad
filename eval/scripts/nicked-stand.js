// A stand that hides the two defects docs/NEXT.md item 1 is about, both
// invisible to every number a reply carried before print_check: it is
// watertight, one body, stands on 87 % of its footprint and fits every bed.
//
// A 60 x 40 x 10 block whose deck leans back 15° to a slot floor, with a
// cable channel through it: the floor runs 1 mm above the channel's ceiling
// at the front and falls through it towards the back, so along the channel
// the material between `floor` and `channel` thins to nothing on a line — a
// feather at 0 mm, which print_check fails. A Ø7 screw `boss` stands on the
// deck, and a `grille` of four Ø5 holes through the deck beside it puts its
// last hole 0.5 mm into the boss's side: a collision, which print_check
// flags. Nothing here names either.
const block = box(60, 40, 10).at(0, 0, 5).tag("block");
const floor = box(80, 80, 20).rotate("x", -15).at(0, 2.588, 15.659).tag("floor");
const channel = box(8, 60, 6).at(-15, 0, 2).tag("channel");
const boss = cylinder(3.5, 8).at(20, -10, 11).tag("boss");
const grille = repeat(cylinder(2.5, 8).at(0, 0, 8), [[-4, -10], [2, -10], [8, -10], [14.3, -10]]).tag("grille");
return block.cut(floor, channel).union(boss).cut(grille);

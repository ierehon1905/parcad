// A cut that takes material from a feature it was not for, with closed
// forms: docs/NEXT.md, item 1. A 30 x 30 x 5 plate carries a 6 x 6 x 8 boss;
// a 2 mm wide slot 3 deep runs across the plate and its floor passes 1 mm
// into the boss's foot. It removes 2 x 30 x 2 = 120 mm³ from the plate, the
// feature it is for, and 1 x 6 x 1 = 6 mm³ from the boss, which is the
// collision: `slot` cuts `boss`, 6 mm³, centred at (2.5, 0, 5.5). The part
// is 4500 + 288 - 126 = 4662 mm³. The tag on the union names both and is
// not reported: "slot cuts body" says nothing "slot cuts boss" does not.
const plate = box(30, 30, 5).at(0, 0, 2.5).tag("plate");
const boss = box(6, 6, 8).at(0, 0, 9).tag("boss");
const slot = box(2, 40, 3).at(3, 0, 4.5).tag("slot");
return plate.union(boss).tag("body").cut(slot);

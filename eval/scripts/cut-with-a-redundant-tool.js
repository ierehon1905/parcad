// A tool wholly inside another tool of the same cut removes nothing of its
// own, but it does meet the material. Judged tool by tool this refused in both
// orders, with two different wrong reasons: listed after the slot it "removed
// nothing and met nowhere"; listed before, it "sealed a void" the slot opens.
// Closed form: 40·40·10 − 10·10·10 = 15000 mm³.
return box(40, 40, 10).cut(box(4, 4, 4), box(10, 10, 20));

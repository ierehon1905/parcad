// A part that meets its envelope only by being turned over.
//
// The brief asks for 70 × 22 × 20 mm — a long shallow slot — and the caddy is
// drawn 20 × 22 × 60, standing up. Compared axis for axis it is 40 mm too
// tall; matched largest to largest, which is the only thing turning a box
// inside a box can do, its 60 takes the 70, its 22 the 22 and its 20 the 20,
// and it fits. That is the judgement `brief` makes and arithmetic down the
// axes does not.
brief({
  envelope: [70, 22, 20],
  budgetCm3: 20,
  holds: ["a pen", "two SD cards"],
  printer: "Bambu A1 mini",
});
const shell = box(20, 22, 60).edges("|Z").expect({ count: 4 }).fillet(3).tag("shell");
return shell.cut(box(14, 16, 56).at(0, 0, 2).tag("well"));

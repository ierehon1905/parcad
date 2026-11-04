// The head-to-head's plate and notch briefs in one part, their numbers
// written as the part's own checks (docs/NEXT.md, "Requirements first"):
// 80 × 50 × 6, four Ø6 through holes 10 mm in from each edge, a 10 × 8
// notch on an 80 mm edge, and a Ø4 hole 3 mm deep. The last two checks are
// the bench's misreads written as checks, and fail on what the part has.
const at = [[-30, -15], [30, -15], [-30, 15], [30, 15]];
const notch = box(10, 10, 10).at(0, 22, 0).tag("notch");
const blind = cylinder(2, 10).at(0, -15, 5);
let plate = box(80, 50, 6).fillet(4, "|Z").cut(notch).cut(blind);
for (const [x, y] of at) plate = plate.cut(cylinder(3, 10).at(x, y, 0));
return {
  plate,
  checks: [
    { size: { is: [80, 50, 6] } },
    { holes: { diameter: 6, through: true, at } },
    { holes: { diameter: 4, through: false, at: [[0, -15]] } },
    { feature: { tag: "notch", size: [10, 8, 6], center: [0, 21, 0] } },
    { holes: { diameter: 12, count: 1 }, why: "the bench's misread: Ø6 as a radius" },
    { feature: { tag: "notch", center: [36, 0, 0] }, why: "the bench's misread: the 50 mm edge" },
  ],
};

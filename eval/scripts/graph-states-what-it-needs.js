// A rounded square tube: its outline has rounded corners and its bore is that
// outline stepped inward, two things parcad 0.0.6 and earlier cannot read. The graph says so in `requires`, and an older host that reads that
// field refuses by name instead of failing on the first curved entry — see
// docs/ARCHITECTURE.md, "A graph says what it needs".
// Stepped in by 2, the bore is 16 x 16 with corners of radius 3 − 2 = 1, so
// the wall is (20² − 4·3²(1 − π/4)) − (16² − 4·1²(1 − π/4)) = 137.133 mm² deep
// 10 mm: 1371.327 mm³.
const r = 3;
const outline = [
  { at: [-10, -10], round: r },
  { at: [10, -10], round: r },
  { at: [10, 10], round: r },
  { at: [-10, 10], round: r },
];
const tube = extrude(outline, 10);
const bore = extrude(inset(outline, 2), 12);
return tube.cut(bore);

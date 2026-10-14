// A curved face on one side: a bar buried to its axis along a plate, its two
// straight root edges eased in. On the plate the matched curvature is zero; on
// the bar the section direction runs round the cylinder, so the blend must
// leave it with curvature 1/6 per mm, not zero. This is the case that proves
// the section reads the face's own curvature rather than assuming a plane.
// The plate is 8 thick so the bar's lower half stays inside it.
const plate = box(40, 40, 8).at(0, 0, 4).tag("plate");
const bar = cylinder(6, 30).rotate("y", 90).at(0, 0, 8).tag("bar");
return union(plate, bar)
  .edges({ between: ["plate", "bar"], curve: "line", parallel: "x" })
  .expect({ count: 2 })
  .smooth(2);

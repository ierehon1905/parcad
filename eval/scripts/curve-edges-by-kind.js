// Edges from section curves answer to their kind. A slot's two half-circle
// ends leave four arc edges (top and bottom rim of each), which
// `curve: "circle"` must find; a parabolic Bézier wall leaves two, which
// `curve: "spline"` must find and nothing straight may join. Both are then
// treated, so a selector that drifts fails aloud on its count.
const slot = extrude(
  [[-10, -5], [10, -5], { through: [15, 0] }, [10, 5], [-10, 5], { through: [-15, 0] }],
  6,
)
  .edges({ curve: "circle" })
  .expect({ count: 4 })
  .chamfer(0.5);

const arch = extrude([[-10, 0], [10, 0], { bezier: [[0, 20]] }], 4)
  .edges({ curve: "spline" })
  .expect({ count: 2 })
  .fillet(0.8);

return { slot, arch: arch.at(0, 0, 10) };

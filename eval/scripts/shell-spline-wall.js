// A turned cup whose wall is a spline, shelled by 1 with the default round
// corners: the volume identity behind `shell` used to hold this to 1e-6 of
// the part, which is under the volume integral's own noise on a revolved
// B-spline wall (2e-6 measured), so every such shell was refused. The wall is
// now measured directly, and reads 1 mm.
return revolve([[0, 0], [10, 0], { spline: [[9, 10]], start: [0, 1], end: [0, 1] }, [10, 20], [0, 20]])
  .shell(1)
  .tag("cup");

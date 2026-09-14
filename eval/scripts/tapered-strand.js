// A tapered pipe against a closed form: a 6 mm strand narrowing to a quarter
// of its size along a 30 mm run, a 90-degree bend of 10 mm, and a 30 mm run —
// a lock of hair, a tail.
//
// The scale falls linearly along the spine's length, 75.708 mm in all
// (30 + 10 pi/2 + 30), and the section stays centred on it, so the volume is
// the integral of pi r(s)^2 — the frustum formula over the whole length, the
// bend included:
//   V = pi * 3^2 * 75.708 * (1 + 0.25 + 0.0625) / 3 = 936.50910 mm^3
// The exact B-rep reads 936.5089 through BRepGProp. A taper that followed each
// edge's own parameter instead of the length would read differently here,
// because the arc and the runs are not the same length.
return pipe([[0, 0, 0], [40, 0, 0], [40, 40, 0]], 6, { bend: 10, taper: 0.25 }).tag("strand");

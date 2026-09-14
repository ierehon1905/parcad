// A spiral horn: a conical helix — radius 12 narrowing to 1.5 over three turns
// of pitch 10 — carrying a 4 mm round section tapered to a tenth of its size.
//
// On a helix the radius and the taper are both linear in the turn angle t,
// so with k = -10.5 / 6pi, c = 30 / 6pi and g(t) = 1 - 0.9 t / 6pi:
//   V = pi 2^2 * integral over [0, 6pi] of g(t)^2 sqrt(k^2 + (12 + k t)^2 + c^2) dt
//     = 811.61951 mm^3
// by quadrature of that closed-form integrand. The exact B-rep reads 811.6192
// through BRepGProp. Had the taper followed arc length instead, the same horn
// would be 614.81 mm^3 — the thick end is where the length is.
//
// The section would cross the axis at the narrow end without the taper (2 mm
// of reach on a 1.5 mm radius); the graph checks both ends, so this builds.
return pipe(
  { helix: { radius: 12, endRadius: 1.5, pitch: 10, turns: 3 } },
  4,
  { taper: 0.1 },
).tag("horn");

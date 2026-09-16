// Two smooth domes 9 mm across, 8.64 mm apart on a plate: their feet overlap by
// 0.36 mm, where both surfaces are nearly flat. OpenCASCADE's fuse returned a
// watertight plate with one dome, 1643.64 mm³ and 7 faces, and no error.
// 8.4 mm apart the same union builds with both, 1687.19 mm³ and 8 faces.
const floor = 2;
const radius = 4.5;
const height = 2.4;
const ease = (x) => x * x * x * (x * (6 * x - 15) + 10);
const z = (r) => floor + height * (1 - ease(r / radius));
const profile = [0.92, 0.82, 0.7, 0.58, 0.46, 0.34, 0.2].map((t) => [t * radius, z(t * radius)]);
const dome = revolve([
  [0, 0],
  [radius, 0],
  [radius, floor],
  { spline: profile, start: [-1, 0], end: [-1, 0] },
  [0, floor + height],
]);
const plate = box(40, 20, floor).at(0, 0, floor / 2);
return plate.union(dome.at(-4.32, 0), dome.at(4.32, 0));

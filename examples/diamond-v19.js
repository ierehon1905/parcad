// diamond v19 — recreated from a Fusion 360 export.
//
// A round brilliant cut, and every one of its 57 faces is a plane: a table,
// 8 star facets, 8 crown mains, 16 upper girdle facets, 16 lower girdle
// facets and 8 pavilion mains meeting in a point (no culet). Fusion built it
// with BoundaryFill — planes arranged in a circular pattern, then the
// enclosed region kept. parcad has no BoundaryFill and does not need one
// here: the solid is convex, so it *is* the intersection of its 57
// half-spaces, and an intersection of rotated boxes is exact in both
// backends.
//
//   volume   196778.06 mm^3 against Fusion's 196778.06   (matches)
//   area     19789.2 mm^2 against Fusion's 19789.2       (matches)
//   bounding box  101.96 x 101.96 x 59.4 mm              (exact; axes remapped)
//   faces    57, all planes, the same six rings
//
// Ground truth is reference/fusion/diamond-v19/ (gitignored). The ring data
// below — tilt of each facet ring's normal from vertical, its plane offset
// from the stone's axis point, and its azimuths — was read off the PLANE
// entities in the STEP export, and the polytope they bound was measured
// against Fusion's own numbers before this script was written.
//
// Two things the export contains that this deliberately does not:
// - a second, identical diamond (CopyPasteBody + Move): a copy, not geometry;
// - a zero-volume cylindrical construction surface (r = 50) left over from
//   the BoundaryFill. The STEP body is additionally trimmed by it into a
//   73-face variant; Fusion's measured body — the specification — is the
//   57-plane solid, which is what this builds.
//
// Axes are remapped from Fusion, whose stone axis was +Y: parcad z = Fusion
// y, so the table faces +Z and the girdle lies in the XY plane.

// Each ring: cosine of the normal's angle to +Z (nz), the plane's signed
// offset d along its own normal (x . n = d), how many facets, and the first
// facet's azimuth. Values are the STEP's own, full precision.
const RINGS = [
  { nz: 1.0, d: 16.2, count: 1, az0: 0 }, // table
  { nz: 0.925444781, d: 24.268313074911, count: 8, az0: 22.5 }, // star
  { nz: 0.833927619, d: 28.134283972459, count: 8, az0: 0 }, // crown mains
  { nz: 0.775787064, d: 31.549739723196, count: 16, az0: 11.25 }, // upper girdle
  { nz: -0.749604959, d: 33.094274660592, count: 16, az0: 11.25 }, // lower girdle
  { nz: -0.762917856, d: 32.958051375205, count: 8, az0: 45 }, // pavilion mains
];

// A half-space is a box big enough to swallow the whole stone from any
// direction, rotated so its +Z face lies in the facet's plane. The stone
// spans ~102 mm; 400 leaves no face of the box near the result.
const S = 400;

const facets = [];
for (const { nz, d, count, az0 } of RINGS) {
  const sin = Math.sqrt(Math.max(0, 1 - nz * nz));
  for (let i = 0; i < count; i++) {
    const az = ((az0 + (i * 360) / count) * Math.PI) / 180;
    const n = { x: sin * Math.cos(az), y: sin * Math.sin(az), z: nz };
    let half = box(S, S, S);
    const tilt = (Math.acos(n.z) * 180) / Math.PI;
    if (tilt > 1e-9) {
      // Rotate +Z onto n about the horizontal axis z x n. No facet points
      // straight down, so the axis never degenerates.
      const len = Math.hypot(-n.y, n.x);
      half = half.rotate({ x: -n.y / len, y: n.x / len, z: 0 }, tilt);
    }
    // Push the box back along the normal until its +Z face is the plane.
    facets.push(half.at((d - S / 2) * n.x, (d - S / 2) * n.y, (d - S / 2) * n.z));
  }
}

return intersect(...facets).tag("stone");

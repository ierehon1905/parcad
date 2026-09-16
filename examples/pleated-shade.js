// A pleated pendant shade: two six-pointed stars, each divided evenly and
// every point pushed along its own normal — out on even points, in on odd —
// in equal increments, frozen where the outline would touch itself; each
// corner of the result rounded; a tween between the two with a sine swell;
// and a loft *surface* through the tweens, thickened into the printable wall.
scriptBudget(4);

const points = 48;          // divisions of each star: one pleat per two points
const increments = 20;      // steps the pleating is pushed out in
const height = 200;
const levels = 21;          // sections the surface is lofted through
const wall = 1.4;           // printed wall, centred on the surface
// The narrowest a pleat may close: two walls and the gap a nozzle needs
// between them, or the thickened walls would run into each other.
const closest = wall + 0.8;
const smoothing = 0.25;     // share of the vertex rule blended in each step
const flare = 0.22;         // the sine swell of the silhouette, as a share of the radius
// Corners are rounded to `tipRadius`; one whose folds are too short for that
// is rounded tighter, but never below `tightest`. A fitted curve turns
// tighter between its points than the points do, and `thicken` refuses a
// wall that would leave its inside skin all but folded, so both stay well
// clear of half the wall.
const tipRadius = 2.2;
const tightest = 1.2;

// A star as a smooth closed curve, divided into equal lengths.
function star(tips, outer, inner, twist) {
  const dense = 4000;
  const raw = Array.from({ length: dense }, (_, i) => {
    const a = (2 * Math.PI * i) / dense;
    const r = (outer + inner) / 2 + ((outer - inner) / 2) * Math.cos(tips * a);
    return [r * Math.cos(a + twist), r * Math.sin(a + twist)];
  });
  const run = [0];
  for (let i = 1; i <= dense; i++) {
    const p = raw[i - 1];
    const q = raw[i % dense];
    run.push(run[i - 1] + Math.hypot(q[0] - p[0], q[1] - p[1]));
  }
  const total = run[dense];
  const out = [];
  let k = 0;
  for (let i = 0; i < points; i++) {
    const target = (i * total) / points;
    while (run[k + 1] < target) k++;
    const f = (target - run[k]) / (run[k + 1] - run[k]);
    const p = raw[k];
    const q = raw[(k + 1) % dense];
    out.push([p[0] + f * (q[0] - p[0]), p[1] + f * (q[1] - p[1])]);
  }
  return out;
}

// Unit normal from the chord between a point's neighbours: outward for an
// anticlockwise ring.
function normals(ring) {
  const n = ring.length;
  return ring.map((_, i) => {
    const a = ring[(i + n - 1) % n];
    const b = ring[(i + 1) % n];
    const tx = b[0] - a[0];
    const ty = b[1] - a[1];
    const len = Math.sqrt(tx * tx + ty * ty);
    return [ty / len, -tx / len];
  });
}

// Radius of the circle through a point and its neighbours, and which side
// the ring turns toward there (+1 outward, -1 inward).
function bend(ring, i) {
  const n = ring.length;
  const p = ring[(i + n - 1) % n];
  const q = ring[i];
  const r = ring[(i + 1) % n];
  const cross = (q[0] - p[0]) * (r[1] - q[1]) - (q[1] - p[1]) * (r[0] - q[0]);
  const a = Math.hypot(q[0] - p[0], q[1] - p[1]);
  const b = Math.hypot(r[0] - q[0], r[1] - q[1]);
  const c = Math.hypot(r[0] - p[0], r[1] - p[1]);
  const radius = Math.abs(cross) < 1e-12 ? Infinity : (a * b * c) / (2 * Math.abs(cross));
  // An anticlockwise ring turning left bends toward its inside.
  return { radius, concave: cross > 0 ? -1 : 1 };
}

// The cubic B-spline vertex rule — Chaikin's corner cutting read back onto
// the points — blended in by `smoothing`.
function rebuild(ring) {
  const n = ring.length;
  return ring.map((q, i) => {
    const p = ring[(i + n - 1) % n];
    const r = ring[(i + 1) % n];
    const sx = p[0] / 8 + (3 * q[0]) / 4 + r[0] / 8;
    const sy = p[1] / 8 + (3 * q[1]) / 4 + r[1] / 8;
    return [q[0] + smoothing * (sx - q[0]), q[1] + smoothing * (sy - q[1])];
  });
}

// Push every point out or in by `amplitude` over `increments` steps. A point
// moving into a bend moves no further than most of that bend's radius, and a
// point whose ring would cross itself or close past `closest` is frozen where
// it was.
function pleat(base, amplitude) {
  const baseNormals = normals(base);
  const offset = new Array(points).fill(0);
  const frozen = new Array(points).fill(false);
  const place = () => rebuild(base.map((p, i) => [p[0] + baseNormals[i][0] * offset[i], p[1] + baseNormals[i][1] * offset[i]]));
  let ring = place();
  for (let step = 0; step < increments; step++) {
    const previous = offset.slice();
    for (let i = 0; i < points; i++) {
      if (frozen[i]) continue;
      const out = i % 2 === 0 ? 1 : -1;
      let move = (out * amplitude) / increments;
      const { radius, concave } = bend(ring, i);
      if (Math.sign(move) === concave) move = Math.sign(move) * Math.min(Math.abs(move), 0.8 * radius);
      offset[i] += move;
    }
    ring = place();
    const touching = new Set();
    for (const [i, j] of outlineCrossings(ring)) {
      for (const k of [i, i + 1, j, j + 1]) touching.add(k % points);
    }
    const gaps = outlineGaps(ring, { ignoreWithin: 2 * closest, upTo: closest });
    gaps.forEach((g, i) => { if (g < closest) touching.add(i); });
    if (touching.size) {
      for (const i of touching) {
        offset[i] = previous[i];
        frozen[i] = true;
      }
      ring = place();
    }
  }
  return ring;
}

// The pleated points as a curve with every corner rounded: an arc tangent to
// the two straight folds either side, then the fold. Every corner is sampled
// the same number of times on every star, so the points pair across the
// sections the surface is lofted through.
function filletRing(ring, radius, roundSamples, foldSamples) {
  const n = ring.length;
  const out = [];
  const corners = ring.map((q, i) => {
    const p = ring[(i + n - 1) % n];
    const r = ring[(i + 1) % n];
    const u = [q[0] - p[0], q[1] - p[1]];
    const v = [r[0] - q[0], r[1] - q[1]];
    const lu = Math.hypot(u[0], u[1]);
    const lv = Math.hypot(v[0], v[1]);
    const du = [u[0] / lu, u[1] / lu];
    const dv = [v[0] / lv, v[1] / lv];
    const turn = Math.acos(Math.max(-1, Math.min(1, du[0] * dv[0] + du[1] * dv[1])));
    // The round may take up to 45 % of the shorter fold either side.
    const round = Math.min(radius, (0.45 * Math.min(lu, lv)) / Math.tan(turn / 2));
    if (round < tightest) {
      throw new Error(`a fold of ${Math.min(lu, lv).toFixed(2)} mm turning ${(turn * 180 / Math.PI).toFixed(0)}° can only be rounded to ${round.toFixed(2)} mm, under ${tightest}; make the pleats shallower`);
    }
    const cut = round * Math.tan(turn / 2);
    const start = [q[0] - du[0] * cut, q[1] - du[1] * cut];
    const end = [q[0] + dv[0] * cut, q[1] + dv[1] * cut];
    const side = du[0] * dv[1] - du[1] * dv[0] > 0 ? 1 : -1;
    const centre = [start[0] - side * du[1] * round, start[1] + side * du[0] * round];
    return { start, end, centre, turn, side, round };
  });
  for (let i = 0; i < n; i++) {
    const c = corners[i];
    const a0 = Math.atan2(c.start[1] - c.centre[1], c.start[0] - c.centre[0]);
    for (let k = 0; k < roundSamples; k++) {
      const a = a0 + c.side * c.turn * (k / roundSamples);
      out.push([c.centre[0] + c.round * Math.cos(a), c.centre[1] + c.round * Math.sin(a)]);
    }
    const next = corners[(i + 1) % n].start;
    for (let k = 0; k < foldSamples; k++) {
      const t = k / foldSamples;
      out.push([c.end[0] + t * (next[0] - c.end[0]), c.end[1] + t * (next[1] - c.end[1])]);
    }
  }
  return out;
}

// The top star is turned π/4 against the bottom one. Points pair by index
// across the sections, so each pleat climbs to the matching pleat of the top
// star and the pleats twist as they rise.
const bottom = filletRing(pleat(star(6, 90, 72, 0), 12), tipRadius, 4, 10);
const top = filletRing(pleat(star(6, 64, 52, Math.PI / 4), 10), tipRadius, 4, 10);

const centroid = (ring) => ring.reduce((c, p) => [c[0] + p[0] / ring.length, c[1] + p[1] / ring.length], [0, 0]);
const sections = Array.from({ length: levels }, (_, k) => {
  const t = k / (levels - 1);
  const z = t * height;
  const tween = bottom.map((p, i) => [p[0] + t * (top[i][0] - p[0]), p[1] + t * (top[i][1] - p[1])]);
  const [cx, cy] = centroid(tween);
  const s = 1 + flare * Math.sin(Math.PI * t);
  return { z, curve: [{ fit: tween.map(([x, y]) => [cx + s * (x - cx), cy + s * (y - cy)]), tolerance: 0.05 }] };
});

const shade = surfaceLoft(sections, { closed: true, smooth: true }).tag("shade");
// A thickened wall's rims lean with the surface; a slab off each end leaves
// the flat ring a printer starts on and a clean top.
const rim = 0.4;
const slab = box(400, 400, 20);
return shade.thicken(wall).cut(slab.at(0, 0, rim - 10), slab.at(0, 0, height - rim + 10));

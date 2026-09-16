// A pleated pendant shade, after BeeGraphy's corrugation node: a star base,
// every sample pushed along its own normal, out and in by turns, grown in equal
// increments and frozen where it meets the curve. Two profiles, a tween between
// them, a sine flare, then one loft with a measured wall.
scriptBudget(4);

const pleats = 18;
const perPleat = 16;
const points = pleats * perPleat;
const height = 180;
const levels = 21;
const starPoints = 8;
const wall = 1.2;
const increments = 20;
// The flare scales every section after its pleats are grown, so the pleats are
// grown to pass at the smallest scale any section reaches.
const flareAt = (t) => 0.72 + 0.36 * Math.sin(Math.PI * (0.15 + 0.7 * t));
const smallest = Math.min(flareAt(0), flareAt(1));
const slot = (2 * wall + 1.2) / smallest;   // the narrowest passage a pleat may leave
const crest = (1.6 * wall) / smallest;      // the tightest crest a wall can wrap
const tolerance = 0.1;

// A star as one smooth closed curve, resampled to equal steps of arc length so
// every pleat is the same width along it.
function starBase(outer, inner, turn) {
  const dense = 2000;
  const raw = Array.from({ length: dense }, (_, i) => {
    const a = (2 * Math.PI * i) / dense;
    const r = (outer + inner) / 2 + ((outer - inner) / 2) * Math.cos(starPoints * (a - turn));
    return [r * Math.cos(a), r * Math.sin(a)];
  });
  const run = [0];
  for (let i = 1; i <= dense; i++) {
    const p = raw[i - 1];
    const q = raw[i % dense];
    run.push(run[i - 1] + Math.hypot(q[0] - p[0], q[1] - p[1]));
  }
  const total = run[dense];
  let k = 0;
  return Array.from({ length: points }, (_, i) => {
    const target = (i * total) / points;
    while (k < dense - 1 && run[k + 1] < target) k++;
    const f = (target - run[k]) / (run[k + 1] - run[k] || 1);
    const p = raw[k];
    const q = raw[(k + 1) % dense];
    return [p[0] + f * (q[0] - p[0]), p[1] + f * (q[1] - p[1])];
  });
}

// Outward normal from the chord between a point's two neighbours.
function normals(ring) {
  const n = ring.length;
  return ring.map((_, i) => {
    const a = ring[(i + n - 1) % n];
    const b = ring[(i + 1) % n];
    const tx = b[0] - a[0];
    const ty = b[1] - a[1];
    const len = Math.hypot(tx, ty) || 1;
    return [ty / len, -tx / len];
  });
}

// Radius of the circle through a point and its neighbours, signed: positive
// where the curve bulges out (a pleat's crest), negative in a valley.
function turn(ring, i) {
  const n = ring.length;
  const p = ring[(i + n - 1) % n];
  const q = ring[i];
  const r = ring[(i + 1) % n];
  const cross = (q[0] - p[0]) * (r[1] - q[1]) - (q[1] - p[1]) * (r[0] - q[0]);
  const a = Math.hypot(q[0] - p[0], q[1] - p[1]);
  const b = Math.hypot(r[0] - q[0], r[1] - q[1]);
  const c = Math.hypot(r[0] - p[0], r[1] - p[1]);
  const radius = (a * b * c) / (2 * Math.abs(cross) || 1e-12);
  return cross >= 0 ? radius : -radius;
}

// Grow the pleats: each point's reach rises in equal steps and stops for good
// at the step that would put it in contact, leave a slot too narrow for two
// walls, or bend a crest tighter than the wall can wrap.
function corrugate(base, amplitude, phase) {
  const normal = normals(base);
  const target = base.map((_, i) => amplitude * Math.cos((2 * Math.PI * i) / perPleat + phase));
  const reach = new Array(points).fill(0);
  const frozen = new Array(points).fill(false);
  const place = () =>
    base.map((p, i) => [p[0] + normal[i][0] * reach[i], p[1] + normal[i][1] * reach[i]]);
  for (let step = 1; step <= increments; step++) {
    for (let i = 0; i < points; i++) if (!frozen[i]) reach[i] = (target[i] * step) / increments;
    let settling = true;
    while (settling) {
      settling = false;
      const ring = place();
      const bad = new Set();
      for (const [s, e] of outlineCrossings(ring)) {
        bad.add(s).add((s + 1) % points).add(e).add((e + 1) % points);
      }
      const gaps = outlineGaps(ring, { ignoreWithin: 2 * slot, upTo: slot });
      for (let i = 0; i < points; i++) {
        if (gaps[i] < slot || (turn(ring, i) > 0 && turn(ring, i) < crest)) bad.add(i);
      }
      for (const i of bad) {
        if (frozen[i]) continue;
        reach[i] = (target[i] * (step - 1)) / increments;
        frozen[i] = true;
        settling = true;
      }
    }
  }
  // Neighbours that froze at different steps leave a staircase no smooth curve
  // follows. Ease each point's share of its reach toward its neighbours', never
  // upward, then hold the result to the same checks the growth used.
  let share = reach.map((r, i) => (target[i] === 0 ? 1 : r / target[i]));
  for (let pass = 0; pass < 6; pass++) {
    share = share.map((m, i) => {
      let sum = 0;
      for (let d = -2; d <= 2; d++) sum += share[(i + d + points) % points];
      return Math.min(m, sum / 5);
    });
  }
  for (let i = 0; i < points; i++) reach[i] = target[i] * share[i];
  const ring = place();
  const gaps = outlineGaps(ring, { ignoreWithin: 2 * slot, upTo: slot });
  for (let i = 0; i < points; i++) {
    if (gaps[i] < slot || (turn(ring, i) > 0 && turn(ring, i) < crest)) {
      throw new Error(`eased pleats fail their own check at point ${i}: gap ${gaps[i].toFixed(2)}, turn ${turn(ring, i).toFixed(2)}`);
    }
  }
  if (outlineCrossings(ring).length) throw new Error("eased pleats cross themselves");
  return ring;
}

const bottom = corrugate(starBase(62, 46, 0), 6, 0);
const top = corrugate(starBase(56, 44, 0), 3, 0);

// The pleats sweep round as they rise: an eased turn from rim to crown, and a
// sideways sway on top of it, so each pleat bends into an S.
const twist = 35;
const sway = 8;
const turnAt = (t) => (Math.PI / 180) * (twist * (0.5 - 0.5 * Math.cos(Math.PI * t)) + sway * Math.sin(2 * Math.PI * t));

function section(t) {
  const flare = flareAt(t);
  const c = Math.cos(turnAt(t));
  const s = Math.sin(turnAt(t));
  const ring = bottom.map((p, i) => {
    const x = flare * ((1 - t) * p[0] + t * top[i][0]);
    const y = flare * ((1 - t) * p[1] + t * top[i][1]);
    return [c * x - s * y, s * x + c * y];
  });
  return { z: t * height, outline: [{ fit: ring, tolerance }] };
}

const sections = Array.from({ length: levels }, (_, k) => section(k / (levels - 1)));
return loft(sections, { smooth: true, wall }).tag("shade");

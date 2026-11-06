/**
 * A part's declared motions as something to scrub: one per distinct hinge or
 * slide a `turns` or `slides` check describes, and the transform it puts its
 * body in at any value of its range. Arithmetic on the reply's own resolved
 * axis, point and range — `MotionReport` — never on the script.
 */

import type { MotionReport } from "./state";

type Vec3 = [number, number, number];

/** One way a body moves, merged from every check that describes it. */
export interface Motion {
  body: string;
  kind: "turn" | "slide";
  /** A unit axis for a turn, a unit direction for a slide. */
  axis: Vec3;
  /** A point the turn's axis passes through; the origin for a slide. */
  at: Vec3;
  /** The drawn pose's end of the range first, in degrees or mm: what a slider runs from and to. */
  from: number;
  to: number;
  /** Where a check on this motion failed: the pose its report names. */
  failsAt: number[];
}

/** Within this, two checks name the same axis or point; the reply rounds directions to a millionth. */
const SAME = 1e-6;

/**
 * The motions a part declares, one per body, kind, axis and point, in the
 * order the checks first name them. Every check over the same hinge widens
 * one range rather than adding a second slider for the same pin.
 */
export function motionsOf(reports: MotionReport[]): Motion[] {
  const motions: (Motion & { lo: number; hi: number })[] = [];
  for (const report of reports) {
    const kind = report.turn ? "turn" : "slide";
    const axis = report.turn?.axis ?? report.slide?.direction;
    const range = report.turn?.degrees ?? report.slide?.mm;
    if (!axis || !range) continue;
    const at: Vec3 = report.turn?.at ?? [0, 0, 0];
    let motion = motions.find(
      (m) => m.body === report.body && m.kind === kind && near(m.axis, axis) && (kind === "slide" || onAxis(m, at)),
    );
    if (!motion) {
      motion = { body: report.body, kind, axis, at, from: 0, to: 0, lo: 0, hi: 0, failsAt: [] };
      motions.push(motion);
    }
    motion.lo = Math.min(motion.lo, ...range);
    motion.hi = Math.max(motion.hi, ...range);
    const worst = report.at_degrees ?? report.at_mm;
    if (report.verdict !== "passed" && worst !== undefined) motion.failsAt.push(worst);
  }
  return motions.map(({ lo, hi, ...motion }) => {
    // The drawn pose is 0, and the slider starts there: a hinge drawn open
    // that closes over [0, -180] runs from 0 to -180, not the other way.
    const fromTop = hi === 0 && lo < 0;
    return { ...motion, from: fromTop ? hi : lo, to: fromTop ? lo : hi };
  });
}

function near(a: Vec3, b: Vec3): boolean {
  return a.every((v, i) => Math.abs(v - b[i]) <= SAME);
}

/** Whether `p` lies on `m`'s axis: two checks may name different points of one pin. */
function onAxis(m: Motion, p: Vec3): boolean {
  const d: Vec3 = [p[0] - m.at[0], p[1] - m.at[1], p[2] - m.at[2]];
  const along = d[0] * m.axis[0] + d[1] * m.axis[1] + d[2] * m.axis[2];
  return Math.hypot(d[0] - along * m.axis[0], d[1] - along * m.axis[1], d[2] - along * m.axis[2]) <= 1e-3;
}

/**
 * Row-major 4×4 taking the body as drawn to `value` along `motion` — the same
 * matrix a report's `worst_pose` is, at its own pose: a right-handed turn about
 * the axis through `at`, or a move along the direction.
 */
export function poseAt(motion: Motion, value: number): number[] {
  const [x, y, z] = motion.axis;
  if (motion.kind === "slide") {
    return [1, 0, 0, x * value, 0, 1, 0, y * value, 0, 0, 1, z * value, 0, 0, 0, 1];
  }
  const angle = (value * Math.PI) / 180;
  const c = Math.cos(angle);
  const s = Math.sin(angle);
  const t = 1 - c;
  const r = [
    [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
    [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
    [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
  ];
  const [px, py, pz] = motion.at;
  const shift = r.map((row, i) => motion.at[i] - (row[0] * px + row[1] * py + row[2] * pz));
  return [...r[0], shift[0], ...r[1], shift[1], ...r[2], shift[2], 0, 0, 0, 1];
}

/**
 * Each moving body's pose with every motion at its value, applied in the
 * order the checks declared them. Bodies that do not move are absent.
 */
export function bodyPoses(motions: Motion[], values: number[]): Map<string, number[]> {
  const poses = new Map<string, number[]>();
  motions.forEach((motion, i) => {
    const pose = poseAt(motion, values[i] ?? 0);
    const before = poses.get(motion.body);
    poses.set(motion.body, before ? multiply(pose, before) : pose);
  });
  return poses;
}

/** `a × b`, both row-major 4×4: `b` first, then `a`. */
function multiply(a: number[], b: number[]): number[] {
  const out = new Array<number>(16).fill(0);
  for (let row = 0; row < 4; row++) {
    for (let col = 0; col < 4; col++) {
      for (let k = 0; k < 4; k++) out[row * 4 + col] += a[row * 4 + k] * b[k * 4 + col];
    }
  }
  return out;
}

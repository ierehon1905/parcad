/**
 * A part's declared motions as something to scrub: mechanisms, each one drive
 * that moves one or more bodies by a turn or a slide, possibly carried by
 * another, and the transform each body is in at any drive value. Arithmetic
 * on the reply's own resolved axes, points and knots — `MotionReport` and the
 * snapshot's `mechanism` — never on the script.
 */

import type { EvaluationSnapshot, MechanismReport, MotionReport } from "./state";

type Vec3 = [number, number, number];

/** One body's share of a mechanism: a turn or a slide by an amount the drive sets, or a ride on another body. */
export interface Move {
  body: string;
  kind: "turn" | "slide" | "rides";
  /** A unit axis for a turn, a unit direction for a slide. */
  axis: Vec3;
  /** A point the turn's axis passes through; the origin for a slide. */
  at: Vec3;
  /** The body this one is carried by: its pose is that body's, then its own joint. */
  parent?: string;
  /** `[drive, value]` in degrees or mm, the drive increasing; straight between. */
  knots: [number, number][];
}

/** Bodies that move as one, on one drive: one slider's worth. */
export interface Mechanism {
  /** What the slider says it moves. */
  label: string;
  /** The drive's range, low to high. */
  drive: [number, number];
  moves: Move[];
  /** Drive values where a check over it fails. */
  failsAt: number[];
}

/** Within this, two checks name the same axis or point; the reply rounds directions to a millionth. */
const SAME = 1e-6;

/**
 * The single-body motions a part's checks declare, one mechanism per body,
 * kind, axis and point, in the order the checks first name them. Every check
 * over the same hinge widens one range rather than adding a second slider for
 * the same pin, and the drive starts at the drawn pose.
 */
export function motionsOf(reports: MotionReport[]): Mechanism[] {
  const found: { move: Move; lo: number; hi: number; fails: number[] }[] = [];
  for (const report of reports) {
    const kind = report.turn ? "turn" : "slide";
    const axis = report.turn?.axis ?? report.slide?.direction;
    const range = report.turn?.degrees ?? report.slide?.mm;
    if (!axis || !range) continue;
    const at: Vec3 = report.turn?.at ?? [0, 0, 0];
    let entry = found.find(
      ({ move }) =>
        move.body === report.body && move.kind === kind && near(move.axis, axis) && (kind === "slide" || onAxis(move, at)),
    );
    if (!entry) {
      entry = { move: { body: report.body, kind, axis, at, knots: [] }, lo: 0, hi: 0, fails: [] };
      found.push(entry);
    }
    entry.lo = Math.min(entry.lo, ...range);
    entry.hi = Math.max(entry.hi, ...range);
    const worst = report.at_degrees ?? report.at_mm;
    if (report.verdict !== "passed" && worst !== undefined) entry.fails.push(worst);
  }
  return found
    .filter(({ lo, hi }) => lo !== hi)
    .map(({ move, lo, hi, fails }) => {
      // The drawn pose is 0, and the drive starts there: a hinge drawn open
      // that closes over [0, -180] runs from 0 to -180, not the other way.
      const [from, to] = hi === 0 && lo < 0 ? [hi, lo] : [lo, hi];
      const ways = found.filter((other) => other.move.body === move.body).length;
      return {
        label: ways === 1 ? move.body : `${move.body} ${axisName(move.axis)}`,
        drive: [0, 1] as [number, number],
        moves: [{ ...move, knots: [[0, from], [1, to]] as [number, number][] }],
        failsAt: fails.map((value) => (value - from) / (to - from)),
      };
    });
}

function near(a: Vec3, b: Vec3): boolean {
  return a.every((v, i) => Math.abs(v - b[i]) <= SAME);
}

/** Whether `p` lies on `m`'s axis: two checks may name different points of one pin. */
function onAxis(m: Move, p: Vec3): boolean {
  const d: Vec3 = [p[0] - m.at[0], p[1] - m.at[1], p[2] - m.at[2]];
  const along = d[0] * m.axis[0] + d[1] * m.axis[1] + d[2] * m.axis[2];
  return Math.hypot(d[0] - along * m.axis[0], d[1] - along * m.axis[1], d[2] - along * m.axis[2]) <= 1e-3;
}

/** `x`, `-y` and so on for a principal axis; nothing for any other. */
function axisName(axis: Vec3): string {
  const principal = axis.findIndex((v) => Math.abs(Math.abs(v) - 1) < SAME);
  return principal < 0 ? "" : `${axis[principal] < 0 ? "−" : ""}${"xyz"[principal]}`;
}

/**
 * The part's mechanism as the snapshot resolves it, one slider for all of it,
 * named for the first body that moves; `undefined` when it declares none.
 */
export function mechanismOf(snapshot: Pick<EvaluationSnapshot, "mechanism" | "checks">): Mechanism | undefined {
  const declared = snapshot.mechanism;
  if (!declared || declared.joints.length === 0) return undefined;
  const moves: Move[] = declared.joints.map((joint) => ({
    body: joint.body,
    kind: joint.kind,
    axis: joint.axis ?? [0, 0, 1],
    at: joint.at ?? [0, 0, 0],
    ...(joint.parent !== undefined && { parent: joint.parent }),
    knots: joint.knots ?? [],
  }));
  const failing = (report: MechanismReport) => report.verdict !== "passed" && report.at_drive !== undefined;
  return {
    label: (moves.find((move) => move.kind !== "rides") ?? moves[0]).body,
    drive: declared.drive,
    moves,
    failsAt: (snapshot.checks?.mechanism ?? []).filter(failing).map((report) => report.at_drive!),
  };
}

/**
 * Every slider a part plays: its mechanism first, then one per single-body
 * motion its checks declare on a body the mechanism does not move. A body the
 * mechanism moves is posed by its row alone, so a `turns` or `slides` check on
 * it adds no row: where it fails at a pose the joint reaches at exactly one
 * drive value, the mechanism's row is marked there.
 */
export function slidersOf(snapshot: Pick<EvaluationSnapshot, "mechanism" | "checks">): Mechanism[] {
  const reports = snapshot.checks?.motions ?? [];
  const mechanism = mechanismOf(snapshot);
  if (!mechanism) return motionsOf(reports);
  const moved = new Set(mechanism.moves.map((move) => move.body));
  const marks: number[] = [];
  for (const report of reports) {
    if (!moved.has(report.body) || report.verdict === "passed") continue;
    const drive = driveOf(mechanism, report);
    if (drive !== undefined) marks.push(drive);
  }
  return [
    { ...mechanism, failsAt: [...mechanism.failsAt, ...marks] },
    ...motionsOf(reports.filter((report) => !moved.has(report.body))),
  ];
}

/**
 * The one drive value at which the mechanism puts `report`'s body in the pose
 * the report is worst at: through the body's own joint, the same pin or
 * direction either way round, and only where the knots pass that value once.
 */
function driveOf(mechanism: Mechanism, report: MotionReport): number | undefined {
  const move = mechanism.moves.find((m) => m.body === report.body);
  const worst = report.at_degrees ?? report.at_mm;
  if (!move || move.parent !== undefined || worst === undefined) return undefined;
  const axis = report.turn?.axis ?? report.slide?.direction;
  if (!axis || move.kind !== (report.turn ? "turn" : "slide")) return undefined;
  if (report.turn && !onAxis(move, report.turn.at)) return undefined;
  const flipped: Vec3 = [-axis[0], -axis[1], -axis[2]];
  const value = near(move.axis, axis) ? worst : near(move.axis, flipped) ? -worst : undefined;
  if (value === undefined) return undefined;
  const found: number[] = [];
  const knots = move.knots;
  for (let i = 1; i < knots.length; i++) {
    const [d0, v0] = knots[i - 1];
    const [d1, v1] = knots[i];
    if (v0 === v1) {
      if (v0 === value) return undefined;
      continue;
    }
    const t = (value - v0) / (v1 - v0);
    if (t < 0 || t > 1) continue;
    const drive = d0 + t * (d1 - d0);
    if (!found.some((d) => Math.abs(d - drive) <= SAME)) found.push(drive);
  }
  return found.length === 1 ? found[0] : undefined;
}

/** The move's degrees or mm at drive value `d`, held at the ends; 0 for a ride. */
export function valueAt(move: Move, d: number): number {
  const knots = move.knots;
  if (move.kind === "rides" || knots.length === 0) return 0;
  if (d <= knots[0][0]) return knots[0][1];
  for (let i = 1; i < knots.length; i++) {
    const [d1, v1] = knots[i];
    if (d <= d1) {
      const [d0, v0] = knots[i - 1];
      return v0 + ((v1 - v0) * (d - d0)) / (d1 - d0);
    }
  }
  return knots[knots.length - 1][1];
}

/**
 * Row-major 4×4 taking the body as drawn to `value` along `move` — the same
 * matrix a report's `worst_pose` is, at its own pose: a right-handed turn about
 * the axis through `at`, or a move along the direction.
 */
export function poseAt(move: Move, value: number): number[] {
  const [x, y, z] = move.axis;
  if (move.kind === "rides") return IDENTITY;
  if (move.kind === "slide") {
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
  const [px, py, pz] = move.at;
  const shift = r.map((row, i) => move.at[i] - (row[0] * px + row[1] * py + row[2] * pz));
  return [...r[0], shift[0], ...r[1], shift[1], ...r[2], shift[2], 0, 0, 0, 1];
}

const IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

/**
 * The drive value that leaves every body as drawn: where a lone motion's range
 * crosses 0, or a mechanism's start, which is drawn as it prints.
 */
export function drawnDrive({ drive, moves }: Mechanism): number {
  const knots = moves[0].knots;
  if (moves.length !== 1 || knots.length !== 2) return drive[0];
  const [[d0, v0], [d1, v1]] = knots;
  return Math.min(drive[1], Math.max(drive[0], d0 + ((d1 - d0) * -v0) / (v1 - v0)));
}

/** The widest a turn steps between two sampled poses of a sweep: fine enough that an arc's lowest point is not missed. */
const SWEEP_STEP_DEGREES = 2;

/**
 * Every pose each moving body passes through, each mechanism swept over its
 * whole drive with the others where they start: what the ground has to lie
 * under. Every knot is a sample, and a turn is stepped at most
 * `SWEEP_STEP_DEGREES` between them, so an arc that dips between two knots
 * still counts.
 */
export function travelPoses(mechanisms: Mechanism[]): Map<string, number[][]> {
  const start = mechanisms.map(drawnDrive);
  const travel = new Map<string, number[][]>();
  const add = (poses: Map<string, number[]>, only?: Set<string>) => {
    for (const [body, pose] of poses) {
      if (only && !only.has(body)) continue;
      const seen = travel.get(body);
      if (seen) seen.push(pose);
      else travel.set(body, [pose]);
    }
  };
  add(bodyPoses(mechanisms, start));
  mechanisms.forEach((mechanism, i) => {
    const moved = new Set(mechanism.moves.map((move) => move.body));
    for (const drive of sweptDrives(mechanism)) {
      const drives = [...start];
      drives[i] = drive;
      add(bodyPoses(mechanisms, drives), moved);
    }
  });
  return travel;
}

/** The drive values a sweep samples: the ends, every knot between, and enough steps between those for every turn. */
function sweptDrives({ drive: [low, high], moves }: Mechanism): number[] {
  const breaks = new Set([low, high]);
  for (const move of moves) for (const [d] of move.knots) if (d > low && d < high) breaks.add(d);
  const sorted = [...breaks].sort((a, b) => a - b);
  const turns = moves.filter((move) => move.kind === "turn");
  const drives = [sorted[0]];
  for (let i = 1; i < sorted.length; i++) {
    const d0 = sorted[i - 1];
    const d1 = sorted[i];
    // Every move is straight between two breaks, so equal steps in the drive are equal steps in each angle.
    const widest = Math.max(0, ...turns.map((move) => Math.abs(valueAt(move, d1) - valueAt(move, d0))));
    const steps = Math.max(1, Math.ceil(widest / SWEEP_STEP_DEGREES));
    for (let k = 1; k <= steps; k++) drives.push(d0 + ((d1 - d0) * k) / steps);
  }
  return drives;
}

/**
 * Each moving body's pose with every mechanism at its drive value: a body's
 * own joint first, then everything the body carrying it does; a body moved by
 * more than one mechanism takes them in order. Bodies that do not move are
 * absent.
 */
export function bodyPoses(mechanisms: Mechanism[], drives: number[]): Map<string, number[]> {
  const poses = new Map<string, number[]>();
  mechanisms.forEach((mechanism, i) => {
    const drive = drives[i] ?? mechanism.drive[0];
    const within = new Map<string, number[]>();
    const poseOf = (body: string, seen: Set<string>): number[] => {
      const known = within.get(body);
      if (known) return known;
      const move = mechanism.moves.find((m) => m.body === body);
      if (!move || seen.has(body)) return IDENTITY;
      const own = poseAt(move, valueAt(move, drive));
      const carried = move.parent === undefined ? own : multiply(poseOf(move.parent, new Set(seen).add(body)), own);
      within.set(body, carried);
      return carried;
    };
    for (const move of mechanism.moves) {
      const pose = poseOf(move.body, new Set());
      const before = poses.get(move.body);
      poses.set(move.body, before ? multiply(pose, before) : pose);
    }
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

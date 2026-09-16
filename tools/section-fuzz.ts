#!/usr/bin/env bun
/**
 * Seeded section outlines across the whole section vocabulary, each with the
 * DSL's verdict on it — the first of the three layers that judge a section.
 *
 *   bun tools/section-fuzz.ts [--seed N] [--per-family N] > outlines.jsonl
 *
 * One JSON object per line: `{ id, family, outline, dsl }`, where `dsl` is
 * `{ ok: true }` or `{ ok: false, message }`. The same seed always writes the
 * same file. `tools/section-fuzz.sh` feeds it to the core and the kernel and
 * counts where the three disagree; docs/SECTION_CHECKS.md says what was found.
 */

import { existsSync, readFileSync } from "node:fs";
import { extrude, type SectionEntry } from "../app/src/dsl";

type P = [number, number];
type Outline = unknown[];

function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

let rand = mulberry32(1);
const uniform = (lo: number, hi: number) => lo + (hi - lo) * rand();
const int = (lo: number, hi: number) => Math.floor(uniform(lo, hi + 1));
const pick = <T>(items: readonly T[]): T => items[Math.floor(rand() * items.length)];
const round6 = (n: number) => Number(n.toPrecision(12));
const pt = (x: number, y: number): P => [round6(x), round6(y)];

/** A simple polygon: points at increasing angle, random radius. */
function star(n: number, scale: number, jag: number): P[] {
  const out: P[] = [];
  const phase = uniform(0, Math.PI * 2);
  for (let i = 0; i < n; i++) {
    const a = phase + (Math.PI * 2 * (i + uniform(-0.3, 0.3))) / n;
    const r = scale * uniform(1 - jag, 1);
    out.push(pt(r * Math.cos(a), r * Math.sin(a)));
  }
  return out;
}

function lobes(n: number, scale: number, count: number, depth: number, noise: number): P[] {
  const out: P[] = [];
  for (let i = 0; i < n; i++) {
    const a = (Math.PI * 2 * i) / n;
    const r = scale * (1 + depth * Math.sin(count * a)) + uniform(-noise, noise);
    out.push(pt(r * Math.cos(a), r * Math.sin(a)));
  }
  return out;
}

const scaled = (points: P[], s: number): P[] => points.map(([x, y]) => pt(x * s, y * s));

/** A U whose two arms end `gap` apart across the slot, drawn anticlockwise. */
function u(gap: number, s: number): P[] {
  return scaled(
    [
      [0, 0],
      [30, 0],
      [30, 20],
      [10 + gap, 20],
      [10 + gap, 10],
      [10, 10],
      [10, 20],
      [0, 20],
    ],
    s,
  ).map(([x, y]) => pt(x, y));
}

const families: Record<string, () => Outline> = {
  "polygon-simple": () => star(int(3, 12), uniform(1, 50), uniform(0, 0.8)),
  "polygon-shuffled": () => {
    const p = star(int(4, 9), 10, 0.5);
    for (let i = p.length - 1; i > 0; i--) {
      const j = int(0, i);
      [p[i], p[j]] = [p[j], p[i]];
    }
    return p;
  },
  "polygon-clockwise": () => star(int(3, 10), 10, 0.6).reverse(),
  // A slot whose arms nearly close: the gap runs from clearly open to exactly shut.
  "near-touching": () => {
    const gap = pick([1, 1e-2, 1e-4, 1e-6, 1e-7, 1e-8, 1e-9, 1e-10, 1e-12, 0, -1e-9, -1e-6, -1]);
    const outline = u(gap, pick([1, 1, 1e-3, 1e3]));
    return outline;
  },
  // An arc or curve whose far side runs toward another edge of the outline.
  "near-touching-curve": () => {
    const gap = pick([1, 1e-2, 1e-4, 1e-6, 1e-8, 0, -1e-6, -1e-2]);
    const kind = pick(["through", "radius", "bezier"]);
    // Rectangle 20 x 10; the top edge from [20, 10] to [0, 10] sags down to y = gap.
    const sag = 10 - gap;
    if (kind === "through") return [[0, 0], [20, 0], [20, 10], { through: pt(10, 10 - sag) }, [0, 10]];
    if (kind === "radius") {
      const r = (100 + sag * sag) / (2 * sag);
      return [[0, 0], [20, 0], [20, 10], { radius: -round6(r) }, [0, 10]];
    }
    // A quadratic Bézier's lowest point is halfway to its control point.
    return [[0, 0], [20, 0], [20, 10], { bezier: [pt(10, 10 - 2 * sag)] }, [0, 10]];
  },
  "near-collinear": () => {
    const eps = pick([1, 1e-3, 1e-6, 1e-9, 1e-12, 0]);
    const side = pick([1, -1]);
    const base: Outline = [[0, 0], [10, side * eps], [20, 0], [20, 10], [0, 10]];
    if (rand() < 0.5) return base;
    base[1] = { at: base[1], round: pick([0.5, 1, 5]) };
    return base;
  },
  "near-collinear-arc": () => {
    const eps = pick([1, 1e-3, 1e-6, 1e-9, 1e-12, 0]);
    return [[0, 0], { through: pt(10, -eps) }, [20, 0], [20, 10], [0, 10]];
  },
  "tiny-edge": () => {
    const len = pick([1e-3, 1e-6, 1e-8, 1e-9, 1e-10, 1e-12, 0]);
    const out: Outline = [[0, 0], [10, 0], [10, len], [10, 10], [0, 10]];
    const twist = rand();
    if (twist < 0.3) out[2] = { at: pt(10, len), round: 0.5 };
    else if (twist < 0.6) out.splice(2, 0, { through: pt(10 + len / 2, len / 2) });
    return out;
  },
  "scale": () => {
    const s = pick([1e-6, 1e-4, 1e-2, 1e2, 1e4]);
    const kind = pick(["polygon", "stadium", "rounded", "spline", "fit", "lobes"]);
    if (kind === "polygon") return scaled(star(int(4, 9), 1, 0.5), s);
    if (kind === "stadium") return [pt(-s, -s), pt(s, -s), { through: pt(2 * s, 0) }, pt(s, s), pt(-s, s), { through: pt(-2 * s, 0) }];
    if (kind === "rounded") return [{ at: pt(-s, -s), round: round6(s / 4) }, { at: pt(s, -s), round: round6(s / 4) }, { at: pt(s, s), round: round6(s / 4) }, { at: pt(-s, s), round: round6(s / 4) }];
    if (kind === "spline") return [{ spline: scaled(lobes(12, 1, 3, 0.2, 0), s) }];
    if (kind === "fit") return [{ fit: scaled(lobes(40, 1, 3, 0.2, 0), s), tolerance: round6(s * 1e-3) }];
    return scaled(lobes(24, 1, 4, 0.3, 0), s);
  },
  "rounded": () => {
    const pts = star(int(3, 8), 10, uniform(0, 0.5));
    return pts.map((p) => (rand() < 0.6 ? { at: p, round: round6(uniform(0.01, 8)) } : p));
  },
  "rounded-reentrant": () => {
    const r = pick([0.5, 2, 4.99, 5, 5.01, 10]);
    return u(10, 1).map((p, i) => (i === 3 || i === 4 || i === 5 || i === 6 ? { at: p, round: r } : p));
  },
  "through-arc": () => {
    const pts = star(int(3, 7), 10, 0.3);
    const k = int(0, pts.length - 1);
    const [a, b] = [pts[k], pts[(k + 1) % pts.length]];
    const mid: P = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
    const bulge = uniform(-25, 25);
    const d = [b[0] - a[0], b[1] - a[1]];
    const l = Math.hypot(d[0], d[1]) || 1;
    const out: Outline = [...pts];
    out.splice(k + 1, 0, { through: pt(mid[0] + (d[1] / l) * bulge, mid[1] - (d[0] / l) * bulge) });
    return out;
  },
  "radius-arc": () => {
    const pts = star(int(3, 7), 10, 0.3);
    const k = int(0, pts.length - 1);
    const [a, b] = [pts[k], pts[(k + 1) % pts.length]];
    const half = Math.hypot(b[0] - a[0], b[1] - a[1]) / 2;
    const factor = pick([1 - 1e-6, 1 - 1e-10, 1, 1 + 1e-10, 1 + 1e-6, 1.01, 2, 10]);
    const out: Outline = [...pts];
    out.splice(k + 1, 0, { radius: round6(half * factor * pick([1, -1])) });
    return out;
  },
  "full-circle": () => {
    const r = pick([1e-3, 1, 1e3]);
    return [pt(r, 0), { through: pt(0, r) }, pt(-r, 0), { through: pt(0, -r) }];
  },
  "spline-between": () => {
    const n = int(1, 6);
    const pts: P[] = [];
    for (let i = 0; i < n; i++) pts.push(pt(uniform(-5, 25), uniform(8, 30)));
    const entry: Record<string, unknown> = { spline: pts };
    if (rand() < 0.3) entry.start = pt(uniform(-1, 1), uniform(-1, 1));
    if (rand() < 0.3) entry.end = pt(uniform(-1, 1), uniform(-1, 1));
    return [[0, 0], [20, 0], [20, 10], entry, [0, 10]];
  },
  // The measured case: a polygon that is simple, a cubic through it that is not.
  "spline-overshoot": () => {
    const pts = lobes(int(20, 200), uniform(20, 70), int(3, 12), uniform(0.05, 0.4), uniform(0, 1.5));
    return [{ spline: pts }];
  },
  "spline-closed-sparse": () => [{ spline: star(int(3, 10), 10, uniform(0, 0.9)) }],
  "bezier": () => {
    const n = int(1, 4);
    const ctrl: P[] = [];
    for (let i = 0; i < n; i++) ctrl.push(pt(uniform(-10, 30), uniform(-15, 30)));
    return [[0, 0], [20, 0], [20, 10], { bezier: ctrl }, [0, 10]];
  },
  "bspline": () => {
    const n = int(1, 8);
    const poles: P[] = [];
    for (let i = 0; i < n; i++) poles.push(pt(uniform(-10, 30), uniform(-15, 30)));
    const entry: Record<string, unknown> = { bspline: poles };
    if (rand() < 0.7) entry.degree = int(1, 6);
    return [[0, 0], [20, 0], [20, 10], entry, [0, 10]];
  },
  "fit-open": () => {
    const n = int(3, 60);
    const noise = pick([0, 0.01, 0.1, 0.5]);
    const waves = int(1, 3);
    const pts: P[] = [];
    for (let i = 1; i <= n; i++) {
      const x = 20 - (20 * i) / (n + 1);
      pts.push(pt(x + uniform(-noise, noise), 10 + 5 * Math.sin((x / 20) * Math.PI * waves) + uniform(-noise, noise)));
    }
    return [[0, 0], [20, 0], [20, 10], { fit: pts, tolerance: pick([0.001, 0.01, 0.05, 0.5]) }, [0, 10]];
  },
  "fit-closed-noisy": () => {
    const pts = lobes(int(30, 250), uniform(20, 70), int(3, 12), uniform(0.05, 0.4), uniform(0, 1.5));
    return [{ fit: pts, tolerance: pick([0.001, 0.01, 0.05, 0.2, 1]) }];
  },
  // A curve that dives through the far side of the outline.
  "curve-crossing": () => {
    const kind = pick(["spline", "bezier", "bspline", "fit"]);
    const depth = pick([5, 9.9, 10.1, 15]);
    const pts: P[] = [pt(15, 10 - depth), pt(10, 12), pt(5, 10 - depth)];
    const entry =
      kind === "spline" ? { spline: pts } : kind === "bezier" ? { bezier: pts } : kind === "bspline" ? { bspline: pts } : { fit: pts, tolerance: 0.01 };
    return [[0, 0], [20, 0], [20, 10], entry, [0, 10]];
  },
};

/** Structural mistakes, each written once: every one is somebody's typo. */
const MALFORMED: Outline[] = [
  [[0, 0], [1, 1]],
  [[0, 0], [10, 0], { through: [5, 5] }],
  [[0, 0], [10, 0], [10, 10], { at: [0, 10], round: 0 }],
  [[0, 0], [10, 0], [10, 10], { at: [0, 10], round: -1 }],
  [[0, 0], [10, 0], [10, 10], { at: [0, 10] }],
  [[0, 0], [10, 0], { radius: 0 }, [10, 10]],
  [[0, 0], [10, 0], { spline: [] }, [10, 10]],
  [[0, 0], [10, 0], { bezier: [] }, [10, 10]],
  [[0, 0], [10, 0], { fit: [], tolerance: 0.1 }, [10, 10]],
  [[0, 0], [10, 0], { fit: [[12, 5]], tolerance: 0 }, [10, 10]],
  [[0, 0], [10, 0], { fit: [[12, 5]] }, [10, 10]],
  [[0, 0], [10, 0], { bspline: [[12, 5]], degree: 0 }, [10, 10]],
  [[0, 0], [10, 0], { bspline: [[12, 5]], degree: 2.5 }, [10, 10]],
  [[0, 0], [10, 0], { bspline: [[12, 5]], degree: 30 }, [10, 10]],
  [[0, 0], [10, 0], { bspline: [[12, 5]], degree: 5 }, [10, 10]],
  [[0, 0], [10, 0], { bezier: Array.from({ length: 30 }, (_, i) => [10 + i / 10, i / 3]) }, [10, 10]],
  [[0, 0], [10, 0], { spline: [[12, 5]], start: [0, 0] }, [10, 10]],
  [[0, 0], [10, 0], { bezier: [[12, 5]], start: [1, 0] }, [10, 10]],
  [[0, 0], [10, 0], { through: [12, 5] }, { through: [12, 6] }, [10, 10]],
  [{ through: [5, -5] }, [0, 0], [10, 0], [10, 10]],
  [{ spline: [[0, 0], [10, 0], [5, 5]], start: [1, 0] }],
  [{ bezier: [[0, 0], [10, 0], [5, 5]] }],
  [{ spline: [[0, 0], [10, 0]] }],
  [{ fit: [[0, 0], [10, 0]], tolerance: 0.1 }],
  [{ fit: [[0, 0], [10, 0], [10, 0], [5, 5]], tolerance: 0.1 }],
  [[0, 0], { spline: [[5, 5], [10, 0]] }],
  [[0, 0], { through: [5, 5] }],
  [[0, 0], [10, 0], { through: [10, 0] }, [10, 10]],
  [[0, 0], [10, 0], [10, 0], [10, 10], [0, 10]],
  [[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]],
  [[0, 0], [10, 0], { at: [10, 10], round: 2 }, { through: [5, 12] }, [0, 10]],
  [[0, 0], [10, 0], { at: [20, 0], round: 1 }, [10, 10]],
  [[0, 0], [10, 0], { at: [0, 0], round: 1 }, [10, 10]],
  [[0, 0], [10, 0], [10, 10], { wobble: 1 }, [0, 10]],
  [[0, 0], [10, 0], [10, 10], [0, 10, 5]],
  [[0, 0], [10, 0], [10, 10], ["0", 10]],
  [[0, 0], [10, 0], [10, 10], null],
  [{ inset: [[0, 0], [10, 0], [10, 10], [0, 10]], by: 1 }],
  [{ inset: [[0, 0], [10, 0], [10, 10], [0, 10]], by: 6 }],
  [{ inset: [[0, 0], [10, 0], [10, 10], [0, 10]], by: 0 }],
  [{ inset: [[0, 0], [10, 10], [10, 0], [0, 10]], by: 1 }],
  [[0, 0], [10, 0], { inset: [[0, 0], [10, 0], [10, 10]], by: 1 }],
  [{ inset: [{ inset: [[0, 0], [10, 0], [10, 10], [0, 10]], by: 1 }], by: 1 }],
  [],
  [[0, 0], [0, 0], [0, 0]],
  [[0, 0], [10, 0], [20, 0]],
];

function dslVerdict(outline: Outline): { ok: true } | { ok: false; message: string } {
  try {
    extrude(outline as SectionEntry[], 2);
    return { ok: true };
  } catch (e) {
    return { ok: false, message: (e as Error).message };
  }
}

function arg(name: string, fallback: number): number {
  const i = process.argv.indexOf(name);
  return i >= 0 ? Number(process.argv[i + 1]) : fallback;
}

const seed = arg("--seed", 1);
const perFamily = arg("--per-family", 100);
const only = process.argv.includes("--family") ? process.argv[process.argv.indexOf("--family") + 1] : undefined;
const lines: string[] = [];
// Outlines found outside this generator — a part's real section — are kept in
// the corpus as `pinned/...` and carried into every run from there.
const corpusPath = new URL("../eval/sections.json", import.meta.url).pathname;
if ((!only || only === "pinned") && existsSync(corpusPath)) {
  for (const c of JSON.parse(readFileSync(corpusPath, "utf8")).cases as Array<{ why: string; from: string; outline: Outline }>) {
    if (!c.from.startsWith("pinned/")) continue;
    lines.push(JSON.stringify({ id: c.from, family: "pinned", why: c.why, outline: c.outline, dsl: dslVerdict(c.outline) }));
  }
}
if (!only || only === "malformed") {
  for (const [i, outline] of MALFORMED.entries()) {
    lines.push(JSON.stringify({ id: `malformed/${i}`, family: "malformed", outline, dsl: dslVerdict(outline) }));
  }
}
for (const [family, make] of Object.entries(families)) {
  if (only && family !== only) continue;
  // Each family has its own stream, so adding one never changes another's outlines.
  let h = seed;
  for (const c of family) h = Math.imul(h ^ c.charCodeAt(0), 2654435761);
  rand = mulberry32(h);
  for (let i = 0; i < perFamily; i++) {
    const outline = make();
    lines.push(JSON.stringify({ id: `${family}/${i}`, family, outline, dsl: dslVerdict(outline) }));
  }
}
process.stdout.write(lines.join("\n") + "\n");

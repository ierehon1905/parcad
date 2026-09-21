/**
 * The parcad modelling language.
 *
 * - Millimetres. Every primitive is centred on the origin; place it with
 *   `.at(x, y, z)`.
 * - Shapes are values: reusing one reuses the node.
 * - A script returns one shape, or named bodies, `return { base, lid }`,
 *   which are measured apart and against each other and never fused.
 *   Selectors, tags and treatments work inside one body.
 * - `checks: [...]` beside the bodies is a list of rules the build must
 *   hold, judged on every build and reported first; see `Check`.
 * - A shape may be a *surface*: faces with no inside and free edges where it
 *   ends. It reports area and free edges instead of a volume; booleans,
 *   fillets, wall thickness and STL refuse it until `.thicken(t)` makes it a
 *   solid. See `surfaceLoft`.
 *
 *     const hole = cylinder(3, 40);
 *     return box(60, 30, 6).cut(hole.at(20, 0, 0), hole.at(-20, 0, 0));
 *
 * @remarks
 * A script builds a description of intent; no geometry is computed in
 * JavaScript. `build()` flattens it to the JSON graph the Rust core evaluates,
 * which is what lets a script outlive a change of kernel. The report measures
 * each body by name and every pair (`clear` by how much, `interfering` by how
 * many mm³); STEP writes one solid per body and STL all of them into one file.
 * Nothing joins one body to another: each sits where its script placed it.
 * Surfaces are Fusion's surface workspace: `surfaceLoft`, `surfaceExtrude`,
 * `surfaceRevolve` and `surfaceSweep` make one; `trim`, `split`,
 * `offsetSurface`, `patch` and `stitchSurfaces` edit and join them; STEP
 * carries one exactly, and named bodies may mix solids and surfaces.
 */

import { isObject, list, parseEdgeSelector, parseVertexSelector, queryShapeError, render, spelled, unknownKeys } from "./selectors";

export type Vec3 = { x: number; y: number; z: number };

type Emit = (childIds: number[]) => Record<string, unknown>;

/** Options shared by the boolean operations. */
export interface BoolOptions {
  /**
   * Radius of the rounding applied where the shapes meet. Zero is a hard corner.
   *
   * Worth knowing: a blend pushes the surface outward by up to `blend / 4` near
   * the seam, so blending two shapes with coincident faces makes the part very
   * slightly larger than nominal on those faces.
   */
  blend?: number;
}

/** The outward normal of a face, as `adjacentTo` spells it. */
export type AxisDirection = "+x" | "-x" | "+y" | "-y" | "+z" | "-z";

/**
 * A topology-aware alternative to the compact directional selector string.
 * Any other key is refused, naming the one it most likely meant.
 */
export interface EdgeQuery {
  /** Match edges created by this named Boolean operation. */
  generatedBy?: string;
  /**
   * `"line"`, `"circle"` (any circular arc) or `"spline"` (anything else:
   * section curves, and the ellipses and intersection curves booleans leave).
   */
  curve?: "line" | "circle" | "spline";
  /**
   * `"hole"`: a circular hole rim, not the rim of an outside boss.
   * `"boundary"`: a free edge, where a surface ends (one face on it); what
   * `.patch()` fills and what a surface report measures as open. A closed
   * solid has none.
   */
  role?: "hole" | "boundary";
  /** Match an edge touching a face with this outward normal. */
  adjacentTo?: { faceNormal: AxisDirection };
  /** Match edge centres at the requested document extrema. */
  at?: Partial<Record<"x" | "y" | "z", "min" | "max">>;
  /**
   * `convex` (an outside corner), `concave` (inside) or `smooth` (no corner:
   * a fillet's boundary). Fillets and chamfers skip smooth edges unless asked
   * for them.
   */
  dihedral?: "convex" | "concave" | "smooth";
  /** A straight edge parallel to this axis: the object form of `|Z`. */
  parallel?: "x" | "y" | "z";
  /** Only edges at least this long, in mm: what keeps a sliver out of a cosmetic pass. */
  longerThan?: number;
  /**
   * Only edges of these tagged features. `{ on: "lip", at: { z: "max" } }` is
   * the lip's own top rim: `at` is then measured among the lip's edges. Tags
   * survive booleans, fillets, chamfers and moves, not offset, shell or
   * intersect.
   */
  on?: string | string[];
  /** Only edges with one face from each of two features: the seam where one meets the other. */
  between?: [string, string];
}

/**
 * A string query or an `EdgeQuery`, resolved anew on every build. `>Z` is
 * furthest in +Z, `<Y` furthest in -Y, `|X` parallel to X, joined by `and`:
 * `>Z and >Y and |X` is the top edge at +Y running along X.
 */
export type EdgeSelector = string | EdgeQuery;

/** A positional query over B-rep vertices for a corner treatment. */
export interface VertexQuery {
  /** Match vertices at the requested document extrema. */
  at?: Partial<Record<"x" | "y" | "z", "min" | "max">>;
}

/** A compact vertex selector such as `>X and >Y and >Z`, or a vertex query. */
export type VertexSelector = string | VertexQuery;

/**
 * What a selector must resolve to, checked on the shape the treatment runs
 * against, so a selector that drifts fails aloud instead of treating other
 * edges. At least one field.
 *
 * - `count` is the number `edges` in an evaluate_part reply's `treatments`
 *   reports for that treatment: paste it from a reply, never count by hand.
 * - `atLeast` and `atMost` are for an expectation written before the count is
 *   known: `{ atLeast: 1 }` says the selector must find something.
 *
 * @example box(40, 20, 10).edges("|Z").expect({ count: 4 }).fillet(2)
 */
export interface EdgeExpectation {
  /** The exact number of selected edges or vertices the selector must match. */
  count?: number;
  /** The fewest the selector may match. */
  atLeast?: number;
  /** The most the selector may match. */
  atMost?: number;
}

/**
 * A surface appearance for `Shape.material`, in glTF's terms. Glossy is low
 * `roughness`.
 *
 * @example box(20, 10, 2).material({ color: "#c9ccd1", metalness: 1, roughness: 0.35 })  // aluminium
 *
 * @remarks
 * More looks: `{ color: "#e8702a", roughness: 0.3, clearcoat: 1 }` lacquered
 * paint, `{ color: "#9fd4ff", opacity: 0.35, roughness: 0.1 }` a clear cover,
 * `{ color: "#202020", emissive: "#30ff60" }` a lit LED.
 */
export interface Material {
  /** `#rrggbb` or `#rgb`. */
  color: string;
  /** 0 mirror-smooth to 1 fully diffuse. Defaults to 0.5. */
  roughness?: number;
  /** 0 plastic or paint to 1 bare metal. Defaults to 0. */
  metalness?: number;
  /** 1 solid, towards 0 see-through; above 0. Defaults to 1. Window only. */
  opacity?: number;
  /** A colour the surface glows with, `#rrggbb` or `#rgb`. Defaults to none. Window only. */
  emissive?: string;
  /** A clear glossy layer over the surface, 0 to 1. Defaults to 0. Window only. */
  clearcoat?: number;
}

/** How a constant-radius edge fillet meets its neighbouring faces. */
export interface FilletOptions {
  /** Tangent (G1) is available now; curvature (G2) is reserved for the exact backend. */
  continuity?: "tangent" | "curvature";
  /** Rolling-ball is available now; setback is reserved for the exact backend. */
  corner?: "rollingBall" | "setback";
}

/** How equal-distance chamfers meet where selected edges share a corner. */
export interface ChamferOptions {
  /** Planar chamfer is available now; miter and blend are reserved for the exact backend. */
  corner?: "chamfer" | "miter" | "blend";
}

/** @internal The position of a treatment call in the editor source. */
export interface SourceLocation {
  /** One-based line in the script, not in the generated Function wrapper. */
  line: number;
  /** One-based column of the treatment method name. */
  column: number;
  method: "fillet" | "chamfer" | "smooth" | "squircle";
}

/** @internal A selected-edge treatment node and the source call that authored it. */
export interface TreatmentSource {
  node: number;
  kind: "fillet" | "chamfer";
  source?: SourceLocation;
}

type TreatmentCall = Omit<TreatmentSource, "node">;

/**
 * A source transform normally supplies this location explicitly. The stack is
 * only a fallback for callers outside the editor, where it may not be portable
 * across JavaScript engines. This is inspection metadata only: it never enters
 * the persisted intent graph or the geometry kernel.
 */
let activeTreatmentSource: SourceLocation | undefined;

/** @internal Wraps an editor treatment call with its syntax-derived location. */
export function __parcadTreatmentSource<T>(source: SourceLocation, run: () => T): T {
  const previous = activeTreatmentSource;
  activeTreatmentSource = source;
  try {
    return run();
  } finally {
    activeTreatmentSource = previous;
  }
}

/**
 * @internal Which of parcad's names a script declared as its own, proved by
 * compiling it: every export is a parameter of every script, so a script that
 * only fails to parse with a name among its parameters is one that declares
 * that name. Nothing is read off the text. Empty when the script parses, or
 * fails for a reason of its own.
 */
/** Notes cap: past these the rest are counted, not kept. */
const NOTES_MAX = 40;
const NOTES_MAX_CHARS = 2000;
let notes: { label: string; value: number | string | number[] }[] = [];
let notesChars = 0;
let notesDropped = 0;

/**
 * Report a value the script computed. It appears in the reply's `notes`,
 * marked "from the script, not measured", and changes nothing about the
 * part.
 *
 * - `value` is a number, a string or a list of numbers; `label` names it.
 * - A note is what the script asked for or worked out, never what was
 *   built: quote a measurement for anything that can be measured.
 * - At most 40 notes and 2000 characters; the rest are counted as
 *   `dropped`.
 *
 * @example
 *     const mouth = 23.25 + 0.5;
 *     note("mouth 2€", mouth);
 *     return box(mouth + 4, 30, 10).cut(box(mouth, 30, 8).at(0, 0, 1));
 *
 * @remarks
 * The coin-holder session (docs/COIN_HOLDER_REVIEW.md, §2.6) encoded a
 * string's character count into a body's Y coordinate to read a number its
 * own script had computed, because nothing else came out of the sandbox.
 * This is that channel, capped so it cannot carry the script back out, and
 * segregated in the reply so a requested number is never read as a measured
 * one.
 */
export function note(label: string, value: number | string | number[]): void {
  if (typeof label !== "string" || !label.trim()) {
    throw new Error(`note() takes a label first, a short name for the value; got ${describeArgument(label)}`);
  }
  const ok =
    (typeof value === "number" && Number.isFinite(value)) ||
    typeof value === "string" ||
    (Array.isArray(value) && value.every((v) => typeof v === "number" && Number.isFinite(v)));
  if (!ok) {
    throw new Error(`note("${label}", ...) takes a finite number, a string or a list of numbers; got ${describeArgument(value)}`);
  }
  const chars = label.length + JSON.stringify(value).length;
  if (notes.length >= NOTES_MAX || notesChars + chars > NOTES_MAX_CHARS) {
    notesDropped += 1;
    return;
  }
  notes.push({ label, value });
  notesChars += chars;
}

/** @internal Hand over the notes a run made, and start the next run empty. */
export function __parcadTakeNotes(): { values: { label: string; value: number | string | number[] }[]; dropped: number } {
  const taken = { values: notes, dropped: notesDropped };
  notes = [];
  notesChars = 0;
  notesDropped = 0;
  return taken;
}

export function __parcadShadowedBuiltins(source: string, names: string[]): string[] {
  const compiles = (parameters: string[]) => {
    try {
      new Function(...parameters, source);
      return true;
    } catch {
      return false;
    }
  };
  if (compiles(names) || !compiles([])) return [];
  // Add the names back one at a time; each one that breaks the compile is a
  // name the script declares.
  const kept: string[] = [];
  const shadowed: string[] = [];
  for (const name of names) {
    if (compiles([...kept, name])) kept.push(name);
    else shadowed.push(name);
  }
  return shadowed;
}

/** @internal The refusal for a script that declares parcad's own names, written for whoever wrote it. */
export function __parcadShadowedBuiltinMessage(shadowed: string[], names: string[]): string {
  const quoted = shadowed.map((name) => `\`${name}\``);
  const list = quoted.length === 1 ? quoted[0] : `${quoted.slice(0, -1).join(", ")} and ${quoted[quoted.length - 1]}`;
  const verb = quoted.length === 1 ? "is one" : `are ${quoted.length}`;
  const example = shadowed[0];
  return (
    `${list} ${verb} of the ${names.length} names parcad puts in every script, so a script cannot declare ` +
    `${quoted.length === 1 ? "it" : "them"} again. Rename the local — \`${example}Mm\`, \`my${example[0].toUpperCase()}${example.slice(1)}\`, ` +
    `or a name saying what it holds — or use parcad's own \`${example}\` instead of declaring one. ` +
    `read_docs (topic dsl, entry \`${example}\`) says what parcad's does.`
  );
}

function treatmentSource(method: SourceLocation["method"]): SourceLocation | undefined {
  if (activeTreatmentSource?.method === method) return activeTreatmentSource;
  const line = new Error().stack
    ?.split("\n")
    .map((frame) => frame.match(/parcad-editor\.js:(\d+):(\d+)/))
    .find((match): match is RegExpMatchArray => match !== null);
  if (!line) return undefined;
  return { line: Number(line[1]) - 2, column: Number(line[2]), method };
}

function isQuery(value: unknown): value is object {
  return typeof value === "object" && value !== null && !Array.isArray(value) && !(value instanceof Shape);
}

function describeValue(value: unknown): string {
  if (value instanceof Shape) return "a shape";
  if (Array.isArray(value)) return "an array";
  return typeof value === "function" ? "a function" : String(value);
}

/** An argument as the caller wrote it, for a refusal that quotes the call back. */
function describeArgument(value: unknown): string {
  if (value instanceof Shape) return "a shape";
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "function") return "a function";
  if (Array.isArray(value) || (typeof value === "object" && value !== null)) {
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
}

/** An axis name or a vector as the vector, or undefined for anything else. */
function axisVector(axis: unknown): Vec3 | undefined {
  if (axis === "x") return { x: 1, y: 0, z: 0 };
  if (axis === "y") return { x: 0, y: 1, z: 0 };
  if (axis === "z") return { x: 0, y: 0, z: 1 };
  if (typeof axis !== "object" || axis === null || Array.isArray(axis)) return undefined;
  const { x, y, z } = axis as Record<string, unknown>;
  if (![x, y, z].every((n) => typeof n === "number" && Number.isFinite(n))) return undefined;
  if (x === 0 && y === 0 && z === 0) return undefined;
  return { x: x as number, y: y as number, z: z as number };
}

function refuseSelector(value: unknown, call: string, entity: "edge" | "corner", examples: string): never {
  const why =
    value === undefined
      ? `needs a selector; leaving it out does not select every ${entity}`
      : `takes a selector, not ${describeValue(value)}`;
  throw new Error(`${call} ${why}. Write ${examples}.`);
}

function assertEdgeSelector(selector: EdgeSelector, call: string, write: (selector: string) => string) {
  if (typeof selector === "string") {
    // The full grammar, not just a non-empty check: this used to accept any
    // non-blank string and let `>Q` survive until the kernel parsed it.
    try {
      parseEdgeSelector(selector);
    } catch (e) {
      // An error is read at the moment of need, where a tool description
      // was read once at the start: the refusal names the tool that parses
      // a selector without building anything. Only where that tool exists —
      // the sandbox agents' scripts run in — and not in the editor.
      if (e instanceof Error && "__parcadNative" in globalThis) {
        e.message += `. check_selector with selector: ${JSON.stringify(selector)} parses one without building anything.`;
      }
      throw e;
    }
    return;
  }
  if (!isQuery(selector)) {
    refuseSelector(
      selector,
      call,
      "edge",
      `${write('">Z"')} for the edges furthest in +Z, or ${write('{ dihedral: "convex" }')} for every outside edge`,
    );
  }
  const shape = queryShapeError(selector as Record<string, unknown>, "edge");
  if (shape) throw new Error(shape);
  if (
    !selector.generatedBy &&
    !selector.curve &&
    !selector.role &&
    !selector.adjacentTo &&
    !selector.dihedral &&
    !selector.parallel &&
    selector.longerThan === undefined &&
    !selector.on &&
    !selector.between &&
    (!selector.at || !Object.values(selector.at).some(Boolean))
  ) {
    throw new Error(
      "edge query is empty; specify generatedBy, curve, role, adjacentTo, at, dihedral, parallel, longerThan, on, or between",
    );
  }
  const names = [
    ...(selector.on === undefined ? [] : Array.isArray(selector.on) ? selector.on : [selector.on]),
    ...(selector.between ?? []),
  ];
  if (names.some((name) => typeof name !== "string" || !name.trim())) {
    throw new Error("on and between must name tagged features");
  }
  if (selector.between !== undefined && selector.between.length !== 2) {
    throw new Error("between takes exactly two feature names, e.g. between: [\"arm\", \"hub\"]");
  }
  if (selector.generatedBy !== undefined && !selector.generatedBy.trim()) {
    throw new Error("generatedBy must name a tagged operation");
  }
  if (selector.curve !== undefined && !["line", "circle", "spline"].includes(selector.curve)) {
    throw new Error(`curve must be "line", "circle" or "spline", not ${JSON.stringify(selector.curve)}`);
  }
  if (selector.role !== undefined && !["hole", "boundary"].includes(selector.role)) {
    throw new Error(`role must be "hole" or "boundary", not ${JSON.stringify(selector.role)}`);
  }
  if (selector.dihedral !== undefined && !["convex", "concave", "smooth"].includes(selector.dihedral)) {
    throw new Error(`dihedral must be "convex", "concave" or "smooth", not ${JSON.stringify(selector.dihedral)}`);
  }
  if (selector.parallel !== undefined && !["x", "y", "z"].includes(selector.parallel)) {
    throw new Error(`parallel must be "x", "y" or "z", not ${JSON.stringify(selector.parallel)}`);
  }
  if (selector.longerThan !== undefined && !(selector.longerThan > 0)) {
    throw new Error("longerThan must be a length in mm greater than zero");
  }
}

/** `shape.fillet(2)` with no selector: the fix is a second argument, in the call as written. */
function assertTreatmentSelector(selector: EdgeSelector, method: "fillet" | "chamfer", size: unknown) {
  const args = typeof size === "number" ? String(size) : method === "fillet" ? "radius" : "distance";
  assertEdgeSelector(selector, `${method}(${args})`, (s) => `.${method}(${args}, ${s})`);
}

function assertVertexSelector(selector: VertexSelector) {
  if (typeof selector === "string") {
    // Previously a regex that collapsed every syntax mistake into the one
    // message about `|X`. The shared parser names the actual fault instead.
    parseVertexSelector(selector);
    return;
  }
  if (!isQuery(selector)) {
    refuseSelector(
      selector,
      "vertices()",
      "corner",
      '.vertices(">X and >Y and >Z") for the corner furthest in +X, +Y and +Z, or .edges({ dihedral: "convex" }) for every outside edge',
    );
  }
  const shape = queryShapeError(selector as Record<string, unknown>, "vertex");
  if (shape) throw new Error(shape);
  if (!selector.at || !Object.values(selector.at).some(Boolean)) {
    throw new Error("vertex query is empty; specify at");
  }
}

function assertEdgeExpectation(expectation: EdgeExpectation) {
  const { count, atLeast, atMost } = expectation ?? {};
  const given = [count, atLeast, atMost].filter((n) => n !== undefined);
  if (!isQuery(expectation) || given.length === 0) {
    throw new Error(
      "expect takes { count: n } — n from `edges` on the treatment in an evaluate_part reply — or " +
        "{ atLeast: n }, { atMost: n } for an expectation written before the count is known. Got " +
        `${describeArgument(expectation)}.`,
    );
  }
  for (const [name, value] of [["count", count], ["atLeast", atLeast], ["atMost", atMost]] as const) {
    if (value !== undefined && !(Number.isInteger(value) && value >= 0)) {
      throw new Error(`expect ${name} must be a whole number, not ${describeArgument(value)}`);
    }
  }
  if (count === 0 || atMost === 0) {
    throw new Error("expect of zero can never hold: an edge treatment must select at least one edge");
  }
  if (atLeast !== undefined && atMost !== undefined && atLeast > atMost) {
    throw new Error(`expect atLeast ${atLeast} is above atMost ${atMost}, which nothing can satisfy`);
  }
  if (count !== undefined && ((atLeast !== undefined && count < atLeast) || (atMost !== undefined && count > atMost))) {
    throw new Error(`expect count ${count} is outside atLeast ${atLeast ?? 0} to atMost ${atMost ?? "any"}`);
  }
}

function assertFilletOptions(options?: FilletOptions) {
  if (!options) return;
  if (options.continuity && options.continuity !== "tangent" && options.continuity !== "curvature") {
    throw new Error('fillet continuity must be "tangent" or "curvature"');
  }
  if (options.corner && options.corner !== "rollingBall" && options.corner !== "setback") {
    throw new Error('fillet corner must be "rollingBall" or "setback"');
  }
}

function assertChamferOptions(options?: ChamferOptions) {
  if (!options) return;
  if (
    options.corner &&
    options.corner !== "chamfer" &&
    options.corner !== "miter" &&
    options.corner !== "blend"
  ) {
    throw new Error('chamfer corner must be "chamfer", "miter", or "blend"');
  }
}

/** A selected edge set, ready for an edge-specific operation. */
export class EdgeSelection {
  /** @internal */
  constructor(
    private readonly owner: Shape,
    private readonly selector: EdgeSelector,
    private readonly expectation?: EdgeExpectation,
  ) {}

  /**
   * Require this selector to resolve to exactly `count` edges.
   *
   * This turns a topology edit that changes the target set into a clear build
   * error instead of silently filleting a different number of edges.
   */
  expect(expectation: EdgeExpectation): EdgeSelection {
    assertEdgeExpectation(expectation);
    return new EdgeSelection(this.owner, this.selector, expectation);
  }

  /**
   * Round the selected logical edges by `radius` millimetres.
   *
   * The default recipe is tangent, rolling-ball. Other valid recipes are kept
   * in the intent graph but the exact backend refuses them until it can produce
   * that geometry exactly.
   */
  fillet(radius: number, options?: FilletOptions): Shape {
    return this.owner.fillet(
      radius,
      this.selector,
      this.expectation,
      options,
      treatmentSource("fillet"),
    );
  }

  /** Bevel the selected edges by an equal distance in millimetres. */
  chamfer(distance: number, options?: ChamferOptions): Shape {
    return this.owner.chamfer(
      distance,
      this.selector,
      this.expectation,
      options,
      treatmentSource("chamfer"),
    );
  }

  /**
   * Request a curvature-continuous (G2) blend.
   *
   * This is the precise CAD term for the "squircle-like" smooth transition.
   * It is kept as authored intent now; the exact backend rejects it until it
   * has a true G2 surface construction.
   */
  smooth(radius: number, options?: Omit<FilletOptions, "continuity">): Shape {
    return this.owner.fillet(
      radius,
      this.selector,
      this.expectation,
      { ...options, continuity: "curvature" },
      treatmentSource("smooth"),
    );
  }

  /** Alias for {@link smooth}; a true squircle is a 2D superellipse, not this 3D blend. */
  squircle(radius: number, options?: Omit<FilletOptions, "continuity">): Shape {
    return this.owner.fillet(
      radius,
      this.selector,
      this.expectation,
      { ...options, continuity: "curvature" },
      treatmentSource("squircle"),
    );
  }

  /**
   * A new surface filling the closed loop of free edges selected: Fusion's
   * Patch.
   *
   * - Select with `{ role: "boundary" }`, narrowed by `at` or `on`. Every
   *   selected edge must be a free edge, and together they must close.
   * - Returns the patch alone. `stitchSurfaces(surface, patch)` joins it.
   * - `tangent: true` makes the fill meet its neighbours smoothly.
   * - A fill more than 0.01 mm off its edges, or 1° off tangent, is refused.
   *
   * @example
   *     // a radius-20 tube 30 tall, patched at both ends: a 37,699 mm³ cylinder
   *     const tube = surfaceExtrude([[20, 0], { through: [0, 20] }, [-20, 0], { through: [0, -20] }], 30, { closed: true });
   *     const lid = tube.edges({ role: "boundary", at: { z: "max" } }).patch();
   *     const floor = tube.edges({ role: "boundary", at: { z: "min" } }).patch();
   *     return stitchSurfaces(tube, lid, floor);
   *
   * @remarks
   * A flat loop is filled with the exact plane, any other with a filling
   * surface through its edges whose distance from them is measured.
   */
  patch(options: { tangent?: boolean } = {}): Shape {
    const extra = Object.keys(options ?? {}).filter((k) => k !== "tangent");
    if (extra.length) throw new Error(`patch takes { tangent }; ${extra.join(", ")} is not an option`);
    const tangent = options.tangent ?? false;
    const selector = this.selector;
    const expect = this.expectation;
    return new Shape(
      ([child]) => ({ op: "patch", child, selector, ...(expect ? { expect } : {}), ...(tangent ? { tangent } : {}) }),
      [this.owner],
    );
  }
}

/** A selected corner-vertex set, expanded to incident edges by the exact backend. */
export class VertexSelection {
  /** @internal */
  constructor(
    private readonly owner: Shape,
    private readonly selector: VertexSelector,
    private readonly expectation?: EdgeExpectation,
  ) {}

  /** Require this selector to resolve to exactly `count` corner vertices. */
  expect(expectation: EdgeExpectation): VertexSelection {
    assertEdgeExpectation(expectation);
    return new VertexSelection(this.owner, this.selector, expectation);
  }

  /** Round every incident edge at the selected corner vertices. */
  fillet(radius: number, options?: FilletOptions): Shape {
    return this.owner.filletVertices(
      radius,
      this.selector,
      this.expectation,
      options,
      treatmentSource("fillet"),
    );
  }

  /** Bevel every incident edge at the selected corner vertices. */
  chamfer(distance: number, options?: ChamferOptions): Shape {
    return this.owner.chamferVertices(
      distance,
      this.selector,
      this.expectation,
      options,
      treatmentSource("chamfer"),
    );
  }

  /** Request a curvature-continuous (G2) corner blend. */
  smooth(radius: number, options?: Omit<FilletOptions, "continuity">): Shape {
    return this.owner.filletVertices(
      radius,
      this.selector,
      this.expectation,
      { ...options, continuity: "curvature" },
      treatmentSource("smooth"),
    );
  }

  /** Alias for {@link smooth}; it describes a G2 blend, not a 2D superellipse. */
  squircle(radius: number, options?: Omit<FilletOptions, "continuity">): Shape {
    return this.owner.filletVertices(
      radius,
      this.selector,
      this.expectation,
      { ...options, continuity: "curvature" },
      treatmentSource("squircle"),
    );
  }
}

function fullHex(name: string, color: string | undefined): string {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color ?? "")?.[1];
  if (!hex) {
    throw new Error(`material ${name} ${JSON.stringify(color)} is not a hex colour; write it as "#c83c32" or "#c33"`);
  }
  return `#${(hex.length === 3 ? [...hex].map((c) => c + c).join("") : hex).toLowerCase()}`;
}

/**
 * A solid, or a step on the way to one. Every method returns a new shape, so a
 * shape can be placed twice or kept as a tool, except `tag` and `material`,
 * which change this one.
 *
 * @remarks
 * A name belongs to the node: a named copy would be a second node built twice.
 */
export class Shape {
  /** @internal Where the script made this shape: the call stack, as the engine prints it. */
  readonly createdAt = new Error().stack;

  /** @internal */
  constructor(
    private readonly emit: Emit,
    private readonly kids: Shape[],
    private name?: string,
    private readonly treatment?: TreatmentCall,
  ) {}

  /** @internal Source call that authored this selected-edge treatment. */
  get treatmentCall(): TreatmentCall | undefined {
    return this.treatment;
  }

  /**
   * Name this shape's faces, for selectors (`on`, `between`) and for
   * `tag_extents` in the report. Changes this shape in place and returns it;
   * a second tag replaces the first everywhere the shape is used.
   *
   * @remarks
   * Tags are the only stable way to point at part of a model: they name the
   * step that made a surface, not its position in a list, so they survive any
   * change of dimensions or ordering.
   */
  tag(name: string): Shape {
    this.name = name;
    return this;
  }

  /**
   * Mark this body as a reference: a real object the part is checked
   * against — a stack of coins, a tipped coin at a mouth — that is not the
   * part. Call it last, on the shape the returned object names.
   *
   * - Built, drawn in blue, measured alone (`named_bodies`, `reference:
   *   true`) and against every body (`between_bodies`), so a `clear` or
   *   `interferes` check can name it.
   * - Never exported, probed, or counted: not in `bodies`, `volume_mm3`,
   *   `size` or `stands_on`.
   * - At least one body must not be a reference.
   *
   * @example
   *     const plate = box(60, 40, 3).at(0, 0, 1.5).tag("plate");
   *     const stack = cylinder(12, 20).at(0, 0, 13.5).reference();
   *     return { plate, stack, checks: [{ clear: ["plate", "stack"], atLeast: 0.2 }] };
   *
   * @remarks
   * A method on the shape rather than a `reference: [...]` list on the
   * returned object: the object then stays names-to-shapes with one data
   * key (`checks`); a name in a list can be misspelt and refer to nothing;
   * and the mark travels with the shape the way `.tag()` and `.material()`
   * do, read off the body the object names. Like a tag it lands on the
   * outermost node, so a reference used to build another shape is refused
   * — call it last.
   */
  reference(): Shape {
    this.isReference = true;
    return this;
  }

  private isReference = false;

  /** @internal */
  get referenceBody(): boolean {
    return this.isReference;
  }

  /**
   * Which way this body prints: the axis that points up on the printer,
   * for a body drawn in its assembled position. Call it last, on the shape
   * the returned object names, the way `.reference()` is.
   *
   * - `"+z"` is as drawn, the default; `"-z"` prints it upside down; `"x"`,
   *   `"-x"`, `"y"`, `"-y"` stand it on a side; `[x, y, z]` is any direction.
   * - `print_check` measures the body's overhang and bed contact in this
   *   orientation, and `save_project` writes `print/<body>.3mf` laid flat in it.
   * - A reference body has no print orientation.
   *
   * @example
   *     const lid = box(40, 40, 3).at(0, 0, 21.5).printedUp("-z");
   *     const base = box(40, 40, 20).at(0, 0, 10);
   *     return { base, lid };
   */
  printedUp(up: PrintAxis | [number, number, number]): Shape {
    const axes: Record<PrintAxis, [number, number, number]> = {
      "+x": [1, 0, 0], x: [1, 0, 0], "-x": [-1, 0, 0],
      "+y": [0, 1, 0], y: [0, 1, 0], "-y": [0, -1, 0],
      "+z": [0, 0, 1], z: [0, 0, 1], "-z": [0, 0, -1],
    };
    let dir: [number, number, number];
    if (typeof up === "string" && up in axes) {
      dir = axes[up];
    } else if (Array.isArray(up) && up.length === 3 && up.every((n) => typeof n === "number" && Number.isFinite(n))) {
      const len = Math.hypot(up[0], up[1], up[2]);
      if (len === 0) throw new Error("printedUp takes a direction with some length; [0, 0, 0] points nowhere");
      dir = [up[0] / len, up[1] / len, up[2] / len];
    } else {
      throw new Error(
        `printedUp takes the axis that points up on the printer — "+z" (as drawn), "-z", "x", "-x", "y", "-y" — or a direction [x, y, z]; got ${describeArgument(up)}`,
      );
    }
    this.printUp = dir;
    return this;
  }

  private printUp?: [number, number, number];

  /** @internal */
  get printedUpDirection(): [number, number, number] | undefined {
    return this.printUp;
  }

  /**
   * How this body looks in the window. Visual only: nothing measured reads it,
   * and an agent's render shows it only with `materials: true`. Changes this
   * shape in place and returns it.
   *
   * - One material per solid: a body wears the outermost one in it, a union
   *   the first operand's, a cutter's never. Two colours are two bodies.
   *
   * @example return { base: box(40, 40, 10).material({ color: "#8a9099", metalness: 0.9 }), lid: box(40, 40, 2).at(0, 0, 7) }
   */
  material(material: Material): Shape {
    const { roughness = 0.5, metalness = 0, opacity = 1, clearcoat = 0 } = material;
    for (const [name, value] of [["roughness", roughness], ["metalness", metalness], ["clearcoat", clearcoat]] as const) {
      if (!(value >= 0 && value <= 1)) {
        throw new Error(`material ${name} ${value} is outside 0 to 1`);
      }
    }
    if (!(opacity > 0 && opacity <= 1)) {
      throw new Error(`material opacity ${opacity} must be above 0 and at most 1`);
    }
    const color = fullHex("colour", material.color);
    const emissive = material.emissive === undefined ? undefined : fullHex("emissive", material.emissive);
    this.look = { color, roughness, metalness, opacity, clearcoat, ...(emissive && { emissive }) };
    return this;
  }

  private look?: Material;

  /** @internal */
  get materialSpec(): Material | undefined {
    return this.look;
  }

  /** @internal */
  get tagName(): string | undefined {
    return this.name;
  }

  /** @internal Shapes this one is built from. */
  get children(): readonly Shape[] {
    return this.kids;
  }

  /** @internal Produce the JSON node, given ids already assigned to children. */
  toNode(childIds: number[]): Record<string, unknown> {
    return this.emit(childIds);
  }

  /** Move by `x`, `y`, `z` millimetres from where the shape currently sits. */
  translate(x: number, y: number, z = 0): Shape {
    if (![x, y, z].every((n) => typeof n === "number" && Number.isFinite(n))) {
      const given = [x, y, z].map(describeArgument).join(", ");
      throw new Error(
        `translate takes three finite distances in mm: .at(x, y, z). Got .at(${given}); an array of ` +
          `coordinates is the OpenSCAD form, and NaN is usually arithmetic on a name that is undefined.`,
      );
    }
    return new Shape(
      ([child]) => ({ op: "translate", child, by: { x, y, z } }),
      [this],
    );
  }

  /** Alias for {@link translate} that reads better when placing a feature. */
  at(x: number, y: number, z = 0): Shape {
    return this.translate(x, y, z);
  }

  /**
   * Rotate about an axis through the origin, in degrees. Positive turns
   * anticlockwise seen from the axis's + end: `.rotate("z", 90)` takes +X to
   * +Y, and `.rotate("x", 90)` takes +Z to -Y, so a Z cylinder lies along Y.
   */
  rotate(axis: Vec3 | "x" | "y" | "z", degrees: number): Shape {
    const a = axisVector(axis);
    // `arguments`, not a rest parameter: the signature is what read_docs
    // shows, and a rest parameter would read as more arguments accepted.
    const givenAll = Array.from(arguments as ArrayLike<unknown>);
    if (givenAll.length > 2 || !a || !Number.isFinite(degrees)) {
      const given = givenAll.map(describeArgument).join(", ");
      const openscad =
        givenAll.length > 2 || typeof axis === "number"
          ? " Three angles is the OpenSCAD form; here it is three calls: .rotate(\"x\", a).rotate(\"y\", b).rotate(\"z\", c)."
          : "";
      throw new Error(
        `rotate takes one axis and one angle in degrees: .rotate("z", 45), or .rotate({ x: 0, y: 1, z: 1 }, 30) ` +
          `for a diagonal axis. Got .rotate(${given}).${openscad}`,
      );
    }
    return new Shape(
      ([child]) => ({ op: "rotate", child, axis: a, degrees }),
      [this],
    );
  }

  /**
   * Reflect in the plane through the origin whose normal is `axis`:
   * `.mirror("x")` flips X, across the YZ plane.
   *
   * - A symmetric part is `union(half, half.mirror("x"))`.
   * - Mirror only a half. A body spanning the plane refills a hole cut on one
   *   side from its own uncut reflection; mirror the features, or cut after
   *   the union.
   *
   * @remarks
   * The refilled hole was measured: the part still builds and nothing warns.
   * A reflection is an isometry, so no surface changes type.
   */
  mirror(axis: Vec3 | "x" | "y" | "z"): Shape {
    const normal = axisVector(axis);
    const givenAll = Array.from(arguments as ArrayLike<unknown>);
    if (givenAll.length > 1 || !normal) {
      const given = givenAll.map(describeArgument).join(", ");
      throw new Error(
        `mirror takes the normal of the plane to reflect in: .mirror("x") flips X across the YZ plane, ` +
          `or .mirror({ x: 1, y: 1, z: 0 }) for a diagonal plane. Got .mirror(${given}).` +
          (givenAll.length > 1 || typeof axis === "number"
            ? " A vector of flags is the OpenSCAD form; here it is one axis name, or one normal."
            : ""),
      );
    }
    return new Shape(([child]) => ({ op: "mirror", child, normal }), [this]);
  }

  /**
   * Resize about the origin: one factor uniformly, three per axis.
   * `sphere(10).scale(2, 1, 0.5)` is an ellipsoid with semi-axes 20, 10, 5.
   *
   * - Factors must be positive; a reflection is `mirror`.
   * - Stretching turns circles into ellipses, which `curve: "circle"` no
   *   longer finds. Fillet after stretching.
   */
  scale(x: number, y = x, z = x): Shape {
    // Only the form is checked here; a negative factor reaches the kernel,
    // whose refusal names mirror() and is what eval/cases measures.
    if (![x, y, z].every((factor) => Number.isFinite(factor) && factor !== 0)) {
      const given = [x, y, z].map(describeArgument).join(", ");
      throw new Error(
        `scale takes one factor, or three: .scale(2) doubles the part, .scale(2, 1, 0.5) stretches it ` +
          `per axis. Got .scale(${given}). An array of factors is the OpenSCAD form, and a reflection is .mirror("x").`,
      );
    }
    return new Shape(
      ([child]) => ({ op: "scale", child, by: { x, y, z } }),
      [this],
    );
  }

  /**
   * Move the surface outward by `distance` (or inward, if negative).
   *
   * Growing also rounds every convex edge by `distance`, which is the cheapest
   * way to break sharp corners on a printed part.
   */
  offset(distance: number): Shape {
    return new Shape(([child]) => ({ op: "offset", child, distance }), [this]);
  }

  /** Hollow this out, leaving a wall of `thickness` inside the current surface. */
  shell(thickness: number): Shape {
    return new Shape(([child]) => ({ op: "shell", child, thickness }), [this]);
  }

  /**
   * This surface made a solid `thickness` mm thick, measured square to the
   * surface: Fusion's Thicken. The way to print, fillet or cut a surface.
   *
   * - `side`: `"both"` (default) centres the wall on the surface, `"out"`
   *   grows it on the normal side, `"in"` on the other.
   * - The normal is to the right of the curve's direction of travel (seen
   *   from +Z for an extrude or a loft): outward for an anticlockwise curve.
   * - Reports the measured thickness as `thickened_mm: { min, max }`; more
   *   than 1 % off anywhere is refused.
   * - Refused where the surface bends tighter than `thickness` on the growing
   *   side (the radius is in the message), and at a crease between faces.
   *
   * @example
   *     // a radius-40 dome, 2 mm thick inward: 2/3 · π · (40³ − 38³) = 19,117 mm³
   *     return surfaceRevolve([[40, 0], { through: [40 * Math.SQRT1_2, 40 * Math.SQRT1_2] }, [0, 40]]).thicken(2, { side: "in" });
   *
   * @remarks
   * The kernel offsets the surface exactly; the solid is then measured along
   * the normal at a grid of points on every face. A crease has no single
   * offset, so it is refused rather than mitred.
   */
  thicken(thickness: number, options: { side?: "both" | "out" | "in" } = {}): Shape {
    if (!(typeof thickness === "number" && Number.isFinite(thickness) && thickness > 0)) {
      throw new Error(`thicken takes a positive thickness in mm; got ${JSON.stringify(thickness)}`);
    }
    const side = options.side ?? "both";
    if (!["both", "out", "in"].includes(side)) {
      throw new Error(`thicken's side is "both", "out" or "in"; got ${JSON.stringify(side)}`);
    }
    return new Shape(
      ([child]) => ({ op: "thicken", child, thickness, ...(side !== "both" ? { side } : {}) }),
      [this],
    );
  }

  /**
   * This surface cut by a tool, keeping the pieces on one side: Fusion's
   * Trim. `keep` depends on the tool (see `TrimKeep`).
   *
   * - A solid tool: `keep: "inside"` or `"outside"`.
   * - A surface tool: `"front"` (its normal side) or `"back"`. It must reach
   *   right across this surface.
   * - A plane, `{ plane: { point: [x, y, z], normal: [x, y, z] } }`:
   *   `"above"` (where the normal points) or `"below"`.
   * - To trim by a curve, trim by `surfaceExtrude(curve, h)` or an extruded
   *   solid.
   * - A tool that misses, or keeps all or none of the surface, is refused.
   *   To keep every piece, use `.split(tool)`.
   *
   * @example
   *     // a 40 × 40 sheet on y = 0, less the strip a radius-10 rod passes: 800 mm²
   *     return surfaceExtrude([[-20, 0], [20, 0]], 40).trim(cylinder(10, 100), { keep: "outside" });
   * @example surfaceExtrude([[-20, 0], [20, 0]], 40).trim({ plane: { point: [0, 0, 10], normal: [0, 0, 1] } }, { keep: "above" })  // 40 × 10
   */
  trim(tool: Shape | { plane: { point: PathPoint; normal: PathPoint } }, options: { keep: TrimKeep }): Shape {
    const keep = options?.keep;
    if (!["inside", "outside", "above", "below", "front", "back"].includes(keep)) {
      throw new Error(
        `trim needs { keep }: "inside" or "outside" a solid tool, "front" or "back" of a surface tool, "above" or "below" a plane; got ${JSON.stringify(keep)}. To cut without removing anything, use .split(tool)`,
      );
    }
    return trimBy(this, tool, keep);
  }

  /**
   * This surface cut along a tool, keeping every piece: Fusion's Split Face.
   *
   * - The tool is a solid, a surface or a plane, as `trim` takes.
   * - The pieces stay joined along the cut, as separate faces that a
   *   selector or a later trim can tell apart.
   *
   * @example
   *     // one 40 × 40 sheet as two 800 mm² faces, split at x = 0
   *     return surfaceExtrude([[-20, 0], [20, 0]], 40).split({ plane: { point: [0, 0, 0], normal: [1, 0, 0] } });
   */
  split(tool: Shape | { plane: { point: PathPoint; normal: PathPoint } }): Shape {
    return trimBy(this, tool, "both");
  }

  /**
   * A new surface `distance` mm from this one along its normal: Fusion's
   * Offset Surface.
   *
   * - Negative `distance` moves against the normal (see `thicken` for which
   *   way the normal points).
   * - Reports the measured distance as `offset_mm: { min, max }`.
   * - Refused where the surface bends tighter than `distance` on the side it
   *   moves toward.
   *
   * @example
   *     // a radius-40 dome moved 5 mm out: 2π · 45² = 12,723 mm²
   *     return surfaceRevolve([[40, 0], { through: [40 * Math.SQRT1_2, 40 * Math.SQRT1_2] }, [0, 40]]).offsetSurface(5);
   */
  offsetSurface(distance: number): Shape {
    if (!(typeof distance === "number" && Number.isFinite(distance) && distance !== 0)) {
      throw new Error(`offsetSurface takes a non-zero distance in mm; got ${JSON.stringify(distance)}`);
    }
    return new Shape(([child]) => ({ op: "offset_surface", child, distance }), [this]);
  }

  /**
   * The edges a selector matches, for a fillet, chamfer or patch to act on.
   *
   * - The selector is required: `.edges()` alone is refused, not read as
   *   every edge. `{ dihedral: "convex" }` is every outside edge.
   * - A string measures against the whole part: `>Z` is the edges furthest
   *   in +Z, `|Z` the straight edges parallel to Z, joined by `and`.
   * - An `EdgeQuery` object narrows by feature (`on`, `between`), corner
   *   angle, curve kind or length.
   * - `.expect({ count })` turns a change in how many edges match into a
   *   build error.
   *
   * @example
   *     // a 20 mm cube with all 12 edges rounded 2 mm: 16³ + 6·16²·2 + 3π·2²·16 + (4/3)π·2³ = 7,805 mm³
   *     return box(20, 20, 20).edges({ dihedral: "convex" }).expect({ count: 12 }).fillet(2);
   *
   * @remarks
   * The selector is intentionally source-facing and is resolved anew on every
   * build. The viewport's `edge@…` IDs are useful for inspection during one
   * evaluation, but are never a durable script reference.
   */
  edges(selector: EdgeSelector): EdgeSelection {
    assertEdgeSelector(selector, "edges()", (s) => `.edges(${s})`);
    return new EdgeSelection(this, selector);
  }

  /**
   * The corners a selector matches, for a treatment of every edge meeting
   * there.
   *
   * - The selector is required: `.vertices()` alone is refused, not read as
   *   every corner. To round every outside edge, use
   *   `.edges({ dihedral: "convex" })`.
   * - `>X and >Y and >Z` is the corner furthest in +X, +Y and +Z. Vertex
   *   strings take only `>` and `<` terms.
   *
   * @example
   *     // one corner of a 20 mm cube rounded 2 mm: 8000 − 3·18·(4 − π) − (8 − (4/3)π) = 7,950 mm³
   *     return box(20, 20, 20).vertices(">X and >Y and >Z").fillet(2);
   *
   * @remarks
   * The exact backend expands each selected vertex to its incident edge set;
   * it never stores a transient viewport vertex ID in the graph.
   */
  vertices(selector: VertexSelector): VertexSelection {
    assertVertexSelector(selector);
    return new VertexSelection(this, selector);
  }

  /** Round the edges matched by an authored selector. */
  fillet(
    radius: number,
    selector: EdgeSelector,
    expectation?: EdgeExpectation,
    options?: FilletOptions,
    source = treatmentSource("fillet"),
  ): Shape {
    assertTreatmentSelector(selector, "fillet", radius);
    if (expectation) assertEdgeExpectation(expectation);
    assertFilletOptions(options);
    return new Shape(
      ([child]) => ({ op: "fillet", child, radius, selector, expect: expectation, recipe: options }),
      [this],
      undefined,
      { kind: "fillet", source },
    );
  }

  /** Bevel the edges matched by an authored selector. */
  chamfer(
    distance: number,
    selector: EdgeSelector,
    expectation?: EdgeExpectation,
    options?: ChamferOptions,
    source = treatmentSource("chamfer"),
  ): Shape {
    assertTreatmentSelector(selector, "chamfer", distance);
    if (expectation) assertEdgeExpectation(expectation);
    assertChamferOptions(options);
    return new Shape(
      ([child]) => ({ op: "chamfer", child, distance, selector, expect: expectation, recipe: options }),
      [this],
      undefined,
      { kind: "chamfer", source },
    );
  }

  /** @internal Round the edge set incident to selected corner vertices. */
  filletVertices(
    radius: number,
    vertices: VertexSelector,
    expectation?: EdgeExpectation,
    options?: FilletOptions,
    source = treatmentSource("fillet"),
  ): Shape {
    assertVertexSelector(vertices);
    if (expectation) assertEdgeExpectation(expectation);
    assertFilletOptions(options);
    return new Shape(
      ([child]) => ({ op: "fillet", child, radius, vertices, expect: expectation, recipe: options }),
      [this],
      undefined,
      { kind: "fillet", source },
    );
  }

  /** @internal Bevel the edge set incident to selected corner vertices. */
  chamferVertices(
    distance: number,
    vertices: VertexSelector,
    expectation?: EdgeExpectation,
    options?: ChamferOptions,
    source = treatmentSource("chamfer"),
  ): Shape {
    assertVertexSelector(vertices);
    if (expectation) assertEdgeExpectation(expectation);
    assertChamferOptions(options);
    return new Shape(
      ([child]) => ({ op: "chamfer", child, distance, vertices, expect: expectation, recipe: options }),
      [this],
      undefined,
      { kind: "chamfer", source },
    );
  }

  /** Fuse this shape with the others; `{ blend: r }` rounds where they meet. */
  union(...rest: (Shape | BoolOptions)[]): Shape {
    return union(this, ...rest);
  }

  /**
   * Subtract each of `tools` from this shape, as one cut: the order they are
   * listed in does not matter. A cutter must pass through, not end on, the
   * faces it opens: make it at least 0.5 mm longer at each open end, or the
   * part keeps a sliver or fails to close.
   */
  cut(...rest: (Shape | BoolOptions)[]): Shape {
    const { shapes, opts } = split(rest);
    const kids = [this, ...shapes];
    return new Shape(
      ([base, ...tools]) => ({
        op: "difference",
        base,
        tools,
        blend: opts.blend ?? 0,
      }),
      kids,
    );
  }

  /** Keep only what this shape and the others all occupy. */
  intersect(...rest: (Shape | BoolOptions)[]): Shape {
    return intersect(this, ...rest);
  }
}

function split(args: (Shape | BoolOptions)[]): {
  shapes: Shape[];
  opts: BoolOptions;
} {
  const shapes: Shape[] = [];
  let opts: BoolOptions = {};
  for (const a of args) {
    if (a instanceof Shape) shapes.push(a);
    else opts = { ...opts, ...a };
  }
  return { shapes, opts };
}

// ---------------------------------------------------------------------------
// Primitives. Every one is centred on the origin — place it with `.at()`.
// ---------------------------------------------------------------------------

/** A rectangular block with the given full extents, centred on the origin. */
export function box(x: number, y: number, z: number): Shape {
  return new Shape(() => ({ op: "cuboid", size: { x, y, z } }), []);
}

/** A ball of radius `r` — a radius, like every other primitive here. */
export function sphere(r: number): Shape {
  return new Shape(() => ({ op: "sphere", r }), []);
}

/**
 * A cylinder along Z with the given radius and full height.
 *
 * Centred like every primitive, in Z as well: `cylinder(3, 20)` runs from
 * z = -10 to z = +10, so a hole through a part that stands on z = 0 is placed
 * at the middle of its own length, not at the face it enters. `holeFor()` does
 * that arithmetic, and overshoots both ends.
 */
export function cylinder(r: number, h: number): Shape {
  return new Shape(() => ({ op: "cylinder", r, h }), []);
}

/**
 * A ring: a circle of radius `minor` swept round Z at radius `major`, with
 * `minor < major`. Both are radii: a 2 mm cord O-ring on a 20 mm ID is
 * `torus(20 / 2 + 2 / 2, 2 / 2)`. `sweep` in degrees, from +X anticlockwise,
 * makes part of a ring.
 */
export function torus(
  major: number,
  minor: number,
  options: { sweep?: number } = {},
): Shape {
  if (!(major > 0) || !(minor > 0)) throw new Error("torus radii must be positive");
  if (minor >= major) {
    throw new Error(
      `a torus with minor radius ${minor} and major radius ${major} passes through its own axis; keep minor < major`,
    );
  }
  const sweep = options.sweep ?? 360;
  if (!(sweep > 0) || sweep > 360) {
    throw new Error(`a torus sweep of ${sweep} degrees is not an arc; give more than 0 and at most 360`);
  }
  // The arc starts at +X and turns anticlockwise, like every other angle here.
  return new Shape(() => ({ op: "torus", major, minor, sweep }), []);
}

/** A point in a revolved section: `[radius, z]`, with radius measured off +Z. */
export type SectionPoint = [number, number];

/**
 * One entry of a section, the outline `extrude`, `revolve`, `loft` and `sweep`
 * take: a list, anticlockwise, closing itself (never repeat the first corner).
 *
 * - One boundary that does not cross itself; a hole is a second shape cut out.
 * - Two corners in a row are a straight edge; between two corners at most one
 *   entry draws the stretch (after the last, the stretch back to the first).
 * - Arcs are exact circles and curves exact B-splines: never approximate a
 *   curve with short lines.
 *
 * @example
 *     // 60 × 30 plate, 4 mm corners: four rounded corners and nothing else
 *     const plate = [{ at: [-30, -15], round: 4 }, { at: [30, -15], round: 4 }, { at: [30, 15], round: 4 }, { at: [-30, 15], round: 4 }];
 *     // slot 24 long, 8 wide: straight sides end at x = ±8, each half circle
 *     // passes through its tip at x = ±12 (the chord's midpoint moved out by the radius)
 *     const slot = [[-8, -4], [8, -4], { through: [12, 0] }, [8, 4], [-8, 4], { through: [-12, 0] }];
 *     return extrude(plate, 3).cut(extrude(slot, 5));
 *
 * @remarks
 * Faces from arcs are cylinders, cones, tori and spheres. A
 * self-touching outline, an arc radius too small for its corners and a round
 * too big for its edges are refused with the numbers that would fit.
 */
export type SectionEntry =
  /** A corner, `[x, y]` (`[radius, z]` in a revolve). */
  | [number, number]
  /**
   * A corner rounded by a tangent arc of radius `round`, between two straight
   * edges. `at` is the sharp corner where the edges would meet, not where the
   * arc starts; the round trims both edges. A 40 × 20 plate with 5 mm corners
   * is exactly four entries, `{ at: [±20, ±10], round: 5 }`, anticlockwise,
   * with no plain corners.
   */
  | { at: [number, number]; round: number }
  /** A circular arc through this point, which lies on it. A full circle is two arcs between two corners. */
  | { through: [number, number] }
  /** The shorter arc of this radius, at least half the chord. Positive bulges out of the section, negative bends in. */
  | { radius: number }
  /**
   * A smooth curve through the points; `start` and `end` are its end
   * directions. `[{ spline: points }]` alone is one closed smooth curve.
   *
   * @remarks
   * A cubic parameterised by chord length, with no curvature at its ends
   * unless `start` and `end` are given.
   */
  | { spline: [number, number][]; start?: [number, number]; end?: [number, number] }
  /** Bézier control points: one is quadratic, two cubic. The curve does not pass through them. */
  | { bezier: [number, number][] }
  /**
   * A clamped B-spline with the corners as its end poles: how a STEP export's
   * poles copy in. `knots`, if given, is the full vector.
   *
   * @remarks
   * Uniform without `knots`. The full vector has poles (counting both
   * corners) + degree + 1 values, the first and last each repeated
   * degree + 1 times.
   */
  | { bspline: [number, number][]; degree?: number; knots?: number[] }
  /**
   * A smooth curve within `tolerance` mm (0.01 to 0.1) of every sampled
   * point: the entry for points that are data. The measured worst distance
   * is reported as `deviation_mm`. Alone, it is one closed smooth loop.
   *
   * @remarks
   * `spline` overshoots between dense points and `bspline` reads them as
   * control points and misses them by up to a millimetre without saying so.
   * The fit uses the fewest poles that hold the tolerance; a closed one is C2
   * where its list starts, like everywhere else. A tolerance that cannot hold,
   * or a fitted curve that crosses itself, is refused naming the tolerance
   * that would hold or the points to thin.
   */
  | { fit: [number, number][]; tolerance: number }
  /** A curve given by a formula, with its own ends: see `CurveEntry`. */
  | CurveEntry
  /** An outline stepped inward; write it as `inset(outline, by)`. */
  | { inset: SectionEntry[]; by: number };

/**
 * A section entry drawn from a formula, `curve(t)` for `t` from `from` to
 * `to`: an involute, a cam law, a spiral.
 *
 * - Its ends are `curve(from)` and `curve(to)`; it needs no corners round
 *   it, and a corner beside it is joined to that end by a straight edge.
 * - The drawn curve is within `tolerance` mm (0.001 is usual) of the function
 *   everywhere. The part reports that bound as `curve_bound_mm`;
 *   `curve_bound` is `certified` when `derivative` and `fourth` are given,
 *   else `estimated`. `deviation_mm` is a check at points, not a bound.
 * - `curve` must be smooth on the range: a cusp is two entries meeting at a
 *   corner.
 *
 * @example
 *     // a quarter circle of radius 10, certified: its fourth derivative has length 10
 *     const arc = { curve: (t) => [10 * Math.cos(t), 10 * Math.sin(t)], derivative: (t) => [-10 * Math.sin(t), 10 * Math.cos(t)], fourth: (a, b) => 10, from: 0, to: Math.PI / 2, tolerance: 0.001 };
 *     return extrude([[0, 0], arc], 5);
 *
 * @remarks
 * The kernel cannot run JavaScript, so the script evaluates the function and
 * draws it as a cubic matching its position and direction at points placed
 * where it bends, adding points until the whole curve is within the
 * tolerance. Estimated: directions come from finite differences and the error
 * is read between the points, so a feature narrower than those samples can
 * hide. Certified: each piece is within `√2 · m · h⁴ / 384` of the function
 * (the cubic Hermite remainder, `h` the piece's length in `t`), as good as the
 * `m` given. `spurGearOutline` draws its flanks this way.
 */
export type CurveEntry = {
  curve: (t: number) => [number, number];
  /** Where `t` starts; `from > to` draws the curve backwards. */
  from: number;
  to: number;
  /** The most the drawn curve may be from the function anywhere, in mm. */
  tolerance: number;
  /** The exact derivative of `curve`, `(t) => [dx, dy]`; checked against `curve`, and refused when it disagrees. */
  derivative?: (t: number) => [number, number];
  /** `(a, b) => m`: at least the length of the fourth derivative anywhere in `[a, b]`; checked against the error the script sees. */
  fourth?: (a: number, b: number) => number;
};

const SECTION_KEYS = ["at", "round", "through", "radius", "spline", "start", "end", "bezier", "bspline", "degree", "knots", "fit", "tolerance", "inset", "by", "curve", "from", "to", "derivative", "fourth"];

function isPair(value: unknown): value is [number, number] {
  return Array.isArray(value) && value.length === 2 && value.every((n) => typeof n === "number" && Number.isFinite(n));
}

/** The highest curve degree the kernel builds: OCCT's `Geom_BSplineCurve::MaxDegree`. */
const MAX_DEGREE = 25;

/** A section that is one closed curve with no corner: `[{ spline }]` or `[{ fit, tolerance }]`. */
function lone(profile: SectionEntry[]): boolean {
  return profile.length === 1 && !Array.isArray(profile[0]) && ("spline" in (profile[0] as object) || "fit" in (profile[0] as object));
}

function isCurveEntry(entry: unknown): entry is CurveEntry {
  return !!entry && typeof entry === "object" && !Array.isArray(entry) && "curve" in entry;
}

/** A value as a message shows it, NaN and Infinity included. */
function shown(value: unknown): string {
  return JSON.stringify(value, (_, v) => (typeof v === "number" && !Number.isFinite(v) ? String(v) : v)) ?? String(value);
}

/** Most pieces one `{ curve }` may be drawn in before it is refused. */
const MAX_CURVE_PIECES = 4096;

type P2 = [number, number];

/**
 * A `{ curve }` entry drawn as a C1 cubic B-spline: cubic Hermite pieces on
 * the function's own points and directions, halved until every piece is
 * within the tolerance — by the Hermite remainder when the entry certifies
 * itself, by the error seen between the points when it does not. What comes
 * back is the curve's two ends and the `bspline` entry between them, carrying
 * the bound and points of the function for the kernel to measure against.
 */
function drawCurve(entry: CurveEntry, label: string): { start: P2; end: P2; drawn: Record<string, unknown> } {
  const { curve, from, to, tolerance, derivative, fourth } = entry;
  if (typeof curve !== "function") {
    throw new Error(`${label}: curve is a function of t returning [x, y], e.g. { curve: (t) => [10 * Math.cos(t), 10 * Math.sin(t)], from: 0, to: Math.PI, tolerance: 0.001 }`);
  }
  if (!(typeof from === "number" && Number.isFinite(from) && typeof to === "number" && Number.isFinite(to)) || from === to) {
    throw new Error(`${label}: a curve runs over t from \`from\` to \`to\`, two different finite numbers; got from ${JSON.stringify(from)}, to ${JSON.stringify(to)}`);
  }
  if (!(typeof tolerance === "number" && tolerance >= 1e-6 && Number.isFinite(tolerance))) {
    throw new Error(`${label}: a curve's tolerance is the most the drawn curve may be from the function anywhere, in mm, at least 0.000001 (the kernel's own precision); usually 0.001. Got ${JSON.stringify(tolerance)}`);
  }
  for (const [key, value] of [["derivative", derivative], ["fourth", fourth]] as const) {
    if (value !== undefined && typeof value !== "function") {
      throw new Error(`${label}: ${key} is a function — derivative: (t) => [dx, dy], fourth: (a, b) => bound — or left out`);
    }
  }
  if (fourth && !derivative) {
    throw new Error(`${label}: fourth certifies a curve only beside its exact derivative; give derivative: (t) => [dx, dy] too, or drop fourth for an estimated bound`);
  }
  const certified = fourth !== undefined;
  const sign = to > from ? 1 : -1;
  const length = Math.abs(to - from);
  const tAt = (s: number) => (s >= length ? to : from + sign * s);
  const point = (s: number): P2 => {
    const t = tAt(s);
    const p = curve(t);
    if (!isPair(p)) {
      throw new Error(`${label}: curve(${t}) returned ${shown(p)}; it must return [x, y], two finite numbers, for every t from ${from} to ${to}`);
    }
    return [p[0], p[1]];
  };
  // Second-order differences that never step outside [from, to], where the
  // function may not be defined.
  const delta = 1e-6 * length;
  const differenced = (s: number): P2 => {
    const start = s - delta < 0 ? s : s + delta > length ? s - 2 * delta : s - delta;
    const weights = s - delta < 0 ? [-3, 4, -1] : s + delta > length ? [1, -4, 3] : [-1, 0, 1];
    const samples = [0, 1, 2].map((k) => point(start + k * delta));
    const along = (axis: 0 | 1) => weights.reduce((sum, w, k) => sum + w * samples[k][axis], 0) / (2 * delta);
    return [along(0), along(1)];
  };
  const slope = (s: number): P2 => {
    if (!derivative) return differenced(s);
    const t = tAt(s);
    const d = derivative(t);
    if (!isPair(d)) {
      throw new Error(`${label}: derivative(${t}) returned ${shown(d)}; it must return [dx, dy], two finite numbers`);
    }
    return [sign * d[0], sign * d[1]];
  };
  if (derivative) {
    for (const f of [0.1, 0.37, 0.5, 0.71, 0.9]) {
      const s = f * length;
      const given = slope(s);
      const seen = differenced(s);
      const scale = Math.hypot(...point(s));
      const allowed = 1e-6 * (1 + Math.hypot(...seen)) + (1e-14 * scale) / delta;
      if (Math.hypot(given[0] - seen[0], given[1] - seen[1]) > allowed) {
        throw new Error(
          `${label}: derivative(${tAt(s)}) is [${given.map((v) => sign * v).join(", ")}], but the curve itself changes at [${seen.map((v) => sign * v).join(", ")}] there. derivative must be the exact derivative of curve with respect to t`,
        );
      }
    }
  }

  const nodes = new Map<number, { p: P2; d: P2 }>();
  const node = (s: number) => {
    let n = nodes.get(s);
    if (!n) {
      n = { p: point(s), d: slope(s) };
      nodes.set(s, n);
    }
    return n;
  };
  type Piece = { s0: number; s1: number; poles: [P2, P2, P2, P2]; bound: number; check: P2[] };
  const pieces: Piece[] = [];
  const initial = certified ? 1 : 8;
  const stack: [number, number][] = [];
  for (let i = initial - 1; i >= 0; i--) {
    stack.push([(length * i) / initial, i + 1 === initial ? length : (length * (i + 1)) / initial]);
  }
  const hermite = (q: [P2, P2, P2, P2], u: number): P2 => {
    const v = 1 - u;
    const w = [v * v * v, 3 * v * v * u, 3 * v * u * u, u * u * u];
    return [
      w[0] * q[0][0] + w[1] * q[1][0] + w[2] * q[2][0] + w[3] * q[3][0],
      w[0] * q[0][1] + w[1] * q[1][1] + w[2] * q[2][1] + w[3] * q[3][1],
    ];
  };
  const probes = certified ? [0.25, 0.5, 0.75] : [0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875];
  while (stack.length > 0) {
    const [s0, s1] = stack.pop()!;
    const h = s1 - s0;
    const a = node(s0);
    const b = node(s1);
    const poles: [P2, P2, P2, P2] = [
      a.p,
      [a.p[0] + (h / 3) * a.d[0], a.p[1] + (h / 3) * a.d[1]],
      [b.p[0] - (h / 3) * b.d[0], b.p[1] - (h / 3) * b.d[1]],
      b.p,
    ];
    let seen = 0;
    let seenAt = s0;
    const check: P2[] = [];
    for (const u of probes) {
      const truth = point(s0 + u * h);
      const drawn = hermite(poles, u);
      const error = Math.hypot(truth[0] - drawn[0], truth[1] - drawn[1]);
      if (error > seen) {
        seen = error;
        seenAt = s0 + u * h;
      }
      if (u === 0.25 || u === 0.5 || u === 0.75) check.push(truth);
    }
    let bound = seen;
    if (fourth) {
      const [ta, tb] = [tAt(s0), tAt(s1)].sort((x, y) => x - y);
      const m = fourth(ta, tb);
      if (!(typeof m === "number" && Number.isFinite(m) && m >= 0)) {
        throw new Error(`${label}: fourth(${ta}, ${tb}) returned ${JSON.stringify(m)}; it must return a finite number, at least 0`);
      }
      bound = (Math.SQRT2 * m * h ** 4) / 384;
      if (seen > bound * (1 + 1e-6) + 1e-12) {
        throw new Error(
          `${label}: the drawn curve is ${seen.toExponential(3)} mm from the function at t = ${tAt(seenAt)}, more than the ${bound.toExponential(3)} mm that fourth(${ta}, ${tb}) = ${m} allows. fourth must be at least the length of the fourth derivative everywhere on [a, b], and derivative the exact derivative; one of them is not`,
        );
      }
    }
    if (bound <= tolerance) {
      pieces.push({ s0, s1, poles, bound, check });
      continue;
    }
    if (pieces.length + stack.length + 2 > MAX_CURVE_PIECES || h < 1e-12 * length) {
      throw new Error(
        `${label}: the curve cannot be held within ${tolerance} mm in ${MAX_CURVE_PIECES} pieces — the piece at t = ${tAt(s0)} to ${tAt(s1)} is still ${bound.toExponential(3)} mm off. Raise the tolerance, or split the range where the curve turns sharply: a cusp or corner inside it is two curve entries meeting at a corner [x, y]`,
      );
    }
    const mid = s0 + h / 2;
    stack.push([mid, s1], [s0, mid]);
  }

  const poles: P2[] = [pieces[0].poles[0], pieces[0].poles[1]];
  const knots = [0, 0, 0, 0];
  for (let k = 0; k + 1 < pieces.length; k++) {
    poles.push(pieces[k].poles[2], pieces[k + 1].poles[1]);
    knots.push(pieces[k].s1, pieces[k].s1);
  }
  const last = pieces[pieces.length - 1];
  poles.push(last.poles[2], last.poles[3]);
  knots.push(length, length, length, length);
  // Room for rounding in the poles, which the remainder does not cover.
  const within = Math.max(...pieces.map((p) => p.bound)) + (certified ? 1e-9 : 0);
  return {
    start: poles[0],
    end: poles[poles.length - 1],
    drawn: {
      bspline: poles.slice(1, -1),
      degree: 3,
      knots,
      within,
      certified,
      check: pieces.flatMap((p) => p.check),
    },
  };
}

/**
 * A section with every `{ curve }` drawn: its two ends become corners, merged
 * with a corner beside it that is the same point, and the curve the
 * `bspline` between them.
 */
function drawCurves(profile: SectionEntry[], what: string, open = false): SectionEntry[] {
  if (!profile.some(isCurveEntry)) return profile;
  const same = (a: P2, b: P2) => Math.hypot(a[0] - b[0], a[1] - b[1]) <= 1e-9 * Math.max(1, Math.hypot(a[0], a[1]));
  const out: SectionEntry[] = [];
  const corner = (p: P2) => {
    const previous = out[out.length - 1];
    if (!(Array.isArray(previous) && same(previous, p))) out.push(p);
  };
  for (const [i, entry] of profile.entries()) {
    if (isCurveEntry(entry)) {
      const { start, end, drawn } = drawCurve(entry, `${what} entry ${i}`);
      corner(start);
      out.push(drawn as unknown as SectionEntry);
      corner(end);
    } else if (Array.isArray(entry)) {
      corner(entry);
    } else {
      out.push(entry);
    }
  }
  const first = out[0];
  const closing = out[out.length - 1];
  if (!open && out.length > 1 && Array.isArray(first) && Array.isArray(closing) && same(first, closing)) out.pop();
  return out;
}

/**
 * Check a section's shape — the kinds of entry and their numbers — so a typo
 * reads as one here. The geometry (arcs that fit, curves that do not cross)
 * is the core's to judge, because a graph can arrive from anywhere.
 */
function checkSection(profile: SectionEntry[], what: string, example: string, open = false): SectionEntry[] {
  if (!Array.isArray(profile)) {
    throw new Error(`${what} must be a list of section entries, e.g. ${example}`);
  }
  const insetAt = profile.findIndex((entry) => entry && typeof entry === "object" && !Array.isArray(entry) && "inset" in entry);
  if (insetAt >= 0) {
    if (open) throw new Error(`${what} is open, and an inset is a closed outline; give the curve's own entries`);
    if (profile.length !== 1) {
      throw new Error(`${what} entry ${insetAt} is an inset, which is a whole section: write inset(outline, d) alone, with the corners and curves inside it`);
    }
    const e = profile[0] as { inset: SectionEntry[]; by: number };
    if (!(typeof e.by === "number" && Number.isFinite(e.by) && e.by > 0)) {
      throw new Error(`${what}: an inset steps inward by a distance in mm, more than 0; got ${JSON.stringify(e.by)}`);
    }
    return [{ inset: checkSection(e.inset, `${what}'s inset outline`, example), by: e.by }];
  }
  let corners = 0;
  let curves = 0;
  for (const [i, entry] of profile.entries()) {
    if (Array.isArray(entry)) {
      if (!isPair(entry)) {
        throw new Error(`${what} entry ${i} must be a corner [x, y] of two finite numbers; got ${JSON.stringify(entry)}`);
      }
      corners++;
      continue;
    }
    if (!entry || typeof entry !== "object") {
      throw new Error(`${what} entry ${i} is ${JSON.stringify(entry)}; an entry is a corner [x, y] or an object such as { through: [x, y] }`);
    }
    const unknown = Object.keys(entry).find((k) => !SECTION_KEYS.includes(k));
    if (unknown === "within" || unknown === "certified" || unknown === "check") {
      throw new Error(
        `${what} entry ${i}: ${unknown} is what a { curve: (t) => [x, y], from, to, tolerance } entry writes about the curve it draws, from the function itself; a script cannot state it. Give the function as a curve entry instead`,
      );
    }
    if (unknown !== undefined) {
      throw new Error(
        `${what} entry ${i} has an unknown key "${unknown}"; a section entry is [x, y], { at, round }, { through }, { radius }, { spline }, { bezier }, { bspline }, { fit, tolerance } or { curve, from, to, tolerance }`,
      );
    }
    const e = entry as Record<string, unknown>;
    if ("curve" in e) {
      const stray = Object.keys(e).find((k) => !["curve", "from", "to", "tolerance", "derivative", "fourth"].includes(k));
      if (stray !== undefined) {
        throw new Error(`${what} entry ${i}: ${stray} does not belong to a curve, which is { curve, from, to, tolerance, derivative?, fourth? }`);
      }
      curves++;
    } else if ("from" in e || "to" in e || "derivative" in e || "fourth" in e) {
      throw new Error(`${what} entry ${i}: from, to, derivative and fourth belong to a { curve: (t) => [x, y] } entry only`);
    } else if ("at" in e || "round" in e) {
      if (!isPair(e.at) || !(typeof e.round === "number" && e.round > 0)) {
        throw new Error(`${what} entry ${i}: a rounded corner is { at: [x, y], round: r } with r > 0`);
      }
      corners++;
    } else if ("through" in e) {
      if (!isPair(e.through)) throw new Error(`${what} entry ${i}: an arc is { through: [x, y] }, the point it passes through`);
    } else if ("radius" in e) {
      if (!(typeof e.radius === "number" && Number.isFinite(e.radius) && e.radius !== 0)) {
        throw new Error(`${what} entry ${i}: an arc is { radius: r } with r non-zero — positive bulges out, negative bends in`);
      }
    } else {
      const key = ["spline", "bezier", "bspline", "fit"].find((k) => k in e);
      if (!key) throw new Error(`${what} entry ${i} names no kind of entry; give through, radius, spline, bezier, bspline or fit`);
      const points = e[key];
      if (!Array.isArray(points) || !points.every(isPair)) {
        throw new Error(`${what} entry ${i}: ${key} takes a list of [x, y] points`);
      }
      if (points.length === 0 && !lone(profile)) {
        throw new Error(`${what} entry ${i}: a ${key} needs at least one point between the corners; with none it is a straight edge, so drop the entry`);
      }
      for (const tangent of ["start", "end"]) {
        if (tangent in e && (key !== "spline" || !isPair(e[tangent]))) {
          throw new Error(`${what} entry ${i}: ${tangent} is a direction [dx, dy] and belongs to a spline only`);
        }
        if (tangent in e && (e[tangent] as number[]).every((c) => c === 0)) {
          throw new Error(`${what} entry ${i}: a spline's ${tangent} direction must be a non-zero vector`);
        }
      }
      if ("knots" in e && (key !== "bspline" || !Array.isArray(e.knots) || !e.knots.every((k) => typeof k === "number" && Number.isFinite(k)))) {
        throw new Error(`${what} entry ${i}: knots is the full knot vector, a list of numbers, and belongs to a bspline only`);
      }
      if ("degree" in e && (key !== "bspline" || !Number.isInteger(e.degree) || (e.degree as number) < 1 || (e.degree as number) > MAX_DEGREE)) {
        throw new Error(`${what} entry ${i}: degree is a whole number from 1 to ${MAX_DEGREE}, usually 3, and belongs to a bspline only`);
      }
      const degree = key === "bezier" ? points.length + 1 : key === "bspline" ? ((e.degree as number | undefined) ?? 3) : 0;
      if (key === "bezier" && degree > MAX_DEGREE) {
        throw new Error(`${what} entry ${i}: a bezier of ${points.length} control points is degree ${degree}, past the ${MAX_DEGREE} the kernel builds; split it at a corner`);
      }
      if (key === "bspline" && points.length + 2 < degree + 1) {
        throw new Error(
          `${what} entry ${i}: a degree ${degree} bspline needs at least ${degree + 1} control points counting the two corners; got ${points.length + 2}. Lower the degree or add control points`,
        );
      }
      if (key === "fit" && !(typeof e.tolerance === "number" && Number.isFinite(e.tolerance) && e.tolerance > 0)) {
        throw new Error(`${what} entry ${i}: a fit is { fit: points, tolerance: t } with t the most the curve may be from any point, in mm, more than 0 — usually 0.01 to 0.1`);
      }
      if ("tolerance" in e && key !== "fit") {
        throw new Error(`${what} entry ${i}: tolerance belongs to a fit only`);
      }
    }
  }
  if (open) {
    if (curves === 0 && corners < 2 && !lone(profile)) {
      throw new Error(`${what} needs at least 2 corners, from one end of the curve to the other, e.g. ${example}`);
    }
    return drawCurves(profile, what, true);
  }
  if (curves === 0 && (corners === profile.length ? corners < 3 : corners === 0 && !lone(profile))) {
    throw new Error(
      corners === profile.length
        ? `${what} needs at least 3 corners, or corners with an arc or curve between them, e.g. ${example}`
        : `${what} has no corners; put arcs and curves between [x, y] corners, or give one { spline: points } or { fit: points, tolerance } alone for a closed smooth curve`,
    );
  }
  return drawCurves(profile, what);
}

/**
 * A closed section in the (radius, z) half-plane, revolved a full turn about Z:
 * a turned part, drawn as the shape a lathe tool leaves. The section is a list
 * of `SectionEntry`; a radiused shoulder is `{ at: [r, z], round: 1 }`.
 *
 * - Radius >= 0 everywhere on the boundary, control points included.
 * - A stepped (re-entrant) section is fine; a self-crossing one is not.
 *
 * @example revolve([[0, 0], [10, 0], [10, 4], { at: [6, 4], round: 1 }, [6, 12], [0, 12]])
 *
 * @remarks
 * What a cone, countersink, stepped shaft, domed cap, O-ring gland or V-groove
 * ring is made of. The core enforces the rules, because a graph can arrive
 * from anywhere; a section crossing the axis would sweep through itself.
 */
export function revolve(profile: SectionEntry[]): Shape {
  const drawn = checkSection(profile, "a revolve section", "revolve([[0, -5], [4, -5], [0, 5]])");
  if (drawn.some((entry) => Array.isArray(entry) && entry[0] < 0)) {
    throw new Error("revolve section radii must be >= 0; mirror the section onto +radius");
  }
  return new Shape(() => ({ op: "revolve", profile: drawn }), []);
}

/**
 * A cone or truncated cone along Z, centred on the origin like every other
 * primitive: `r1` at the bottom, `r2` at the top.
 *
 * A cone *is* a revolved triangle, which is why this is four lines rather than
 * a kernel primitive. `cone(r, 0, h)` is a point; `cone(r, r, h)` is a cylinder
 * built the long way round, and `cylinder()` is the one to use for that.
 */
export function cone(r1: number, r2: number, h: number): Shape {
  if (r1 < 0 || r2 < 0) throw new Error("cone radii must be >= 0");
  if (r1 === 0 && r2 === 0) throw new Error("a cone needs at least one non-zero radius");
  if (h <= 0) throw new Error("cone height must be positive");
  // Anticlockwise in (radius, z), and the two zero-radius cases drop the
  // degenerate point rather than emitting a repeated one.
  const section: SectionPoint[] = [[0, -h / 2]];
  if (r1 > 0) section.push([r1, -h / 2]);
  if (r2 > 0) section.push([r2, h / 2]);
  section.push([0, h / 2]);
  return revolve(section);
}

/**
 * A countersink cutter for a screw head: the frustum a drill leaves, sized by
 * the head diameter and the included angle.
 *
 * 90° is the ISO metric countersink and the default here; 82° is the imperial
 * one. The cutter is returned positioned so its wide end sits at z = 0 — cut it
 * where the hole breaks out, `.at(x, y, faceZ)`.
 */
export function countersink(head: number | string, includedAngle = 90): Shape {
  // A thread designation looks the size up rather than making the caller carry
  // it: `countersink("M5")` is the same call with the number it stands for.
  const headDia = typeof head === "string" ? fastener(head).csink : head;
  if (headDia <= 0) throw new Error("countersink head diameter must be positive");
  if (includedAngle <= 0 || includedAngle >= 180) {
    throw new Error("countersink included angle must be between 0 and 180 degrees");
  }
  // The cone's half-angle is half the included angle, so the depth follows from
  // the head radius: depth = r / tan(half).
  const r = headDia / 2;
  const slope = Math.tan((includedAngle / 2) * (Math.PI / 180));
  const depth = r / slope;

  // Built 0.5 mm taller than the countersink actually is, and widened along its
  // own taper to match, so the tool crosses the face instead of ending exactly
  // on it. A cutter coplanar with the surface it cuts leaves a zero-thickness
  // sliver: here it cost the rim entirely — OCCT merged the cone's flat top
  // into the face and the rim stopped being an edge the cut had generated.
  // The section at z = 0 still has radius r, so the visible countersink is the
  // size the drawing calls for.
  const over = 0.5;
  return cone(0, r + over * slope, depth + over).at(0, 0, over - (depth + over) / 2);
}

// ---------------------------------------------------------------------------
// Fasteners. The numbers a machinist knows by heart, and a script otherwise
// writes as an unchecked literal.
// ---------------------------------------------------------------------------

/** How loosely a clearance hole is drilled, per ISO 273. */
export type Fit = "close" | "normal" | "free";

/**
 * ISO metric coarse fasteners by designation, M2 to M20 (`M2_5` is M2.5), in mm:
 * `pitch` (ISO 261 coarse), `tap` drill, `close`/`normal`/`free` clearance
 * (ISO 273), `head` of a socket head cap screw (ISO 4762), `csink` of a 90°
 * countersunk screw (ISO 10642). A size not listed is refused, never guessed.
 */
export const METRIC_FASTENERS: Record<
  string,
  { pitch: number; tap: number; close: number; normal: number; free: number; head: number; csink: number }
> = {
  M2: { pitch: 0.4, tap: 1.6, close: 2.2, normal: 2.4, free: 2.6, head: 3.8, csink: 4.0 },
  M2_5: { pitch: 0.45, tap: 2.05, close: 2.7, normal: 2.9, free: 3.1, head: 4.5, csink: 5.0 },
  M3: { pitch: 0.5, tap: 2.5, close: 3.2, normal: 3.4, free: 3.6, head: 5.5, csink: 6.0 },
  M4: { pitch: 0.7, tap: 3.3, close: 4.3, normal: 4.5, free: 4.8, head: 7.0, csink: 8.0 },
  M5: { pitch: 0.8, tap: 4.2, close: 5.3, normal: 5.5, free: 5.8, head: 8.5, csink: 10.0 },
  M6: { pitch: 1, tap: 5.0, close: 6.4, normal: 6.6, free: 7.0, head: 10.0, csink: 12.0 },
  M8: { pitch: 1.25, tap: 6.8, close: 8.4, normal: 9.0, free: 10.0, head: 13.0, csink: 16.0 },
  M10: { pitch: 1.5, tap: 8.5, close: 10.5, normal: 11.0, free: 12.0, head: 16.0, csink: 20.0 },
  M12: { pitch: 1.75, tap: 10.2, close: 13.0, normal: 13.5, free: 14.5, head: 18.0, csink: 24.0 },
  M16: { pitch: 2, tap: 14.0, close: 17.0, normal: 17.5, free: 18.5, head: 24.0, csink: 32.0 },
  M20: { pitch: 2.5, tap: 17.5, close: 21.0, normal: 22.0, free: 24.0, head: 30.0, csink: 40.0 },
};

function fastener(thread: string) {
  const key = thread.trim().toUpperCase().replace(".", "_");
  const entry = METRIC_FASTENERS[key];
  if (!entry) {
    throw new Error(
      `no fastener data for ${thread}. Known sizes: ${Object.keys(METRIC_FASTENERS)
        .map((k) => k.replace("_", "."))
        .join(", ")}. Add it to METRIC_FASTENERS, or write the diameter out and say where it came from`,
    );
  }
  return entry;
}

/** Drill diameter for a coarse-pitch tapped hole: `tapDrill("M6")` is 5.0. */
export function tapDrill(thread: string): number {
  return fastener(thread).tap;
}

/** Clearance hole diameter, ISO 273: `clearance("M6")` is 6.6, close fit 6.4. */
export function clearance(thread: string, fit: Fit = "normal"): number {
  return fastener(thread)[fit];
}

/**
 * Counterbore for a socket head cap screw: the diameter to bore and how deep.
 *
 * The bore is the head diameter plus 1 mm of drill clearance, and the depth is
 * the head height, which for these screws is the thread diameter itself.
 */
export function counterbore(thread: string): { diameter: number; depth: number } {
  const entry = fastener(thread);
  const nominal = Number(thread.trim().toUpperCase().replace("M", "").replace("_", "."));
  return { diameter: entry.head + 1, depth: nominal };
}

/**
 * A hole cutter along Z for a named fastener, entering at z = 0 going down:
 * place it at the face it enters. `holeFor("M6", 12)` is a blind clearance hole
 * 12 mm deep; `tapped` drills for a thread a machinist will tap; `through`
 * also clears the far face. It already overshoots by 0.5 mm. A printed or
 * modelled thread is `threadedHole`.
 *
 * @example box(60, 30, 6).cut(holeFor("M5", 6, { through: true }).at(20, 0, 3))  // entering the top face
 *
 * @remarks
 * The cutter overshoots the entry face by 0.5 mm, and the far face with
 * `through`: a tool ending exactly on a face leaves a zero-thickness sliver,
 * and one starting on it can cost the rim a selector was going to reach.
 */
export function holeFor(
  thread: string,
  depth: number,
  options: { fit?: Fit; tapped?: boolean; through?: boolean } = {},
): Shape {
  if (!(depth > 0)) throw new Error("hole depth must be positive");
  const diameter = options.tapped
    ? tapDrill(thread)
    : clearance(thread, options.fit ?? "normal");
  const over = 0.5;
  const past = options.through ? over : 0;
  const length = depth + over + past;
  return cylinder(diameter / 2, length).at(0, 0, over - length / 2);
}

/** A thread named from `METRIC_FASTENERS` ("M8", coarse pitch) or given outright. */
export type ThreadSize = string | { diameter: number; pitch: number };

/** Options shared by `threadedRod` and `threadedHole`. */
export interface ThreadOptions {
  /** `"right"` (the default, and nearly every screw) or `"left"`. */
  hand?: "right" | "left";
  /**
   * Radial allowance in mm, default 0 (the ISO basic profile). A rod shrinks
   * and a hole grows by it. A printed pair gives it to both: `c` on each sits
   * `c` apart across the flanks and `2c` at crest and root.
   *
   * @remarks
   * 0.2 is a starting point for FDM with a 0.4 mm nozzle, not a measured fit;
   * tune it on the printer. `between_bodies` reads the gap back.
   */
  clearance?: number;
  /** A fine pitch in place of the coarse one a named size carries: `{ pitch: 1 }` on "M8". */
  pitch?: number;
}

function threadForm(size: ThreadSize, options: ThreadOptions, fn: string) {
  let diameter: number;
  let pitch: number;
  if (typeof size === "string") {
    const entry = fastener(size);
    diameter = Number(size.trim().toUpperCase().replace("M", "").replace("_", "."));
    pitch = options.pitch ?? entry.pitch;
  } else if (size && typeof size === "object") {
    diameter = size.diameter;
    pitch = options.pitch ?? size.pitch;
  } else {
    throw new Error(`${fn} takes a size such as "M8" or { diameter, pitch }; got ${JSON.stringify(size)}`);
  }
  if (!(diameter > 0) || !(pitch > 0)) {
    throw new Error(`${fn} needs a positive diameter and pitch; got diameter ${diameter}, pitch ${pitch}`);
  }
  const clearance = options.clearance ?? 0;
  if (!(clearance >= 0)) {
    throw new Error(`${fn} clearance is a radial allowance of 0 or more; got ${clearance}`);
  }
  if (options.hand !== undefined && options.hand !== "right" && options.hand !== "left") {
    throw new Error(`${fn} hand is "right" or "left"; got ${JSON.stringify(options.hand)}`);
  }
  return { diameter, pitch, clearance, hand: options.hand ?? "right" };
}

/**
 * An external ISO 68-1 thread along Z, centred on the origin like `cylinder`,
 * squared off at both ends; union a head or shank onto it. `size` is `"M8"`
 * (coarse pitch) or `{ diameter, pitch }`.
 *
 * - The tooth crosses +X at z = 0 of the rod's own frame. A rod and a
 *   `threadedHole` of the same size mate only when their frames are a whole
 *   number of pitches apart along Z.
 * - A nut reading **interfering** is out of phase (move it a whole pitch) or
 *   its hole stops short (place the hole's frame on the face it enters).
 *
 * @example
 *     const bolt = threadedRod("M6", 20, { clearance: 0.2 });   // frame at z = 0
 *     const nut = box(10, 10, 5).at(0, 0, 2.5)                  // z 0 to 5
 *       .cut(threadedHole("M6", 5, { through: true, clearance: 0.2 }).at(0, 0, 5));
 *     return { bolt, nut };                                      // 5 mm = 5 pitches: in phase
 *
 * @remarks
 * The basic profile: 60° flanks, a core at the basic minor diameter (6.647 mm
 * for M8), flats of P/8 at the crest and P/4 at the root. A 1/4"-20 tripod
 * screw is `threadedRod({ diameter: 6.35, pitch: 25.4 / 20 }, 9)`. A hole out
 * of phase can also be turned about Z by 360° × offset / pitch. The hole
 * cutter spans z = +0.5 down to −depth (−depth − 0.5 with `through`); moving a
 * nut off the thread fixes neither mistake. `between_bodies` reads a good pair
 * clear by the clearance across the flanks. The kernel measures every thread
 * against its closed-form volume and refuses one more than 2e-5 off.
 */
export function threadedRod(size: ThreadSize, length: number, options: ThreadOptions = {}): Shape {
  if (!(length > 0)) throw new Error("threadedRod length must be positive");
  const t = threadForm(size, options, "threadedRod");
  return new Shape(() => ({
    op: "thread",
    diameter: t.diameter,
    pitch: t.pitch,
    from: -length / 2,
    to: length / 2,
    ...(t.hand === "left" ? { hand: "left" } : {}),
    ...(t.clearance > 0 ? { shift: -t.clearance } : {}),
  }), []);
}

/**
 * The cutter for an internal screw thread along Z, entering at z = 0 going
 * down like `holeFor`: a nut, a cap, a threaded boss. Cut it, do not union it.
 * Its tooth crosses +X at the entry face, so a `threadedRod` mates a whole
 * number of pitches away along Z. A hole a machinist will tap is
 * `holeFor(size, depth, { tapped: true })`.
 *
 * @example box(20, 20, 10).cut(threadedHole("M8", 10, { through: true }).at(0, 0, 5))  // a nut
 *
 * @remarks
 * It cuts a bore at the minor diameter and the thread out to the major, and
 * overshoots the entry face by 0.5 mm (and the far face with `through`).
 */
export function threadedHole(
  size: ThreadSize,
  depth: number,
  options: ThreadOptions & { through?: boolean } = {},
): Shape {
  if (!(depth > 0)) throw new Error("threadedHole depth must be positive");
  const t = threadForm(size, options, "threadedHole");
  const over = 0.5;
  const past = options.through ? over : 0;
  return new Shape(() => ({
    op: "thread",
    diameter: t.diameter,
    pitch: t.pitch,
    from: -(depth + past),
    to: over,
    ...(t.hand === "left" ? { hand: "left" } : {}),
    ...(t.clearance > 0 ? { shift: t.clearance } : {}),
  }), []);
}

// ---------------------------------------------------------------------------
// Real objects. The things a holder wraps, measured once, so a part fits the
// object rather than a model's recollection of it. The V holder for a 16"
// MacBook Pro shipped with two radii guessed in a script; the guess now lives
// here, labelled as one, where a better measurement replaces it everywhere.
// ---------------------------------------------------------------------------

/** A device's body: the box it is, and the two radii that make a cutter fit it. */
export interface DeviceBody {
  /** Along the front edge, mm. */
  length: number;
  /** Front to back, mm. */
  width: number;
  /** Closed, mm. */
  thickness: number;
  /** Its corners in plan, mm. */
  cornerRadius: number;
  /** Its top and bottom edges, mm. */
  edgeRadius: number;
  /** Where each number came from. The radii are the honest weak point. */
  source: string;
}

/**
 * Devices a holder is likely to wrap. Sizes are the maker's published ones;
 * the radii are read off photographs and say so, to about a millimetre.
 * Exported so a missing device is obviously absent rather than approximated.
 */
export const DEVICES: Record<string, DeviceBody> = {
  "macbook-air-13": {
    length: 304.1, width: 215.0, thickness: 11.3, cornerRadius: 11, edgeRadius: 3,
    source: "Apple tech specs, M2/M3 (2022-24); radii from photographs, +-1 mm",
  },
  "macbook-air-15": {
    length: 340.4, width: 237.6, thickness: 11.5, cornerRadius: 11, edgeRadius: 3,
    source: "Apple tech specs, M2/M3 (2023-24); radii from photographs, +-1 mm",
  },
  "macbook-pro-14": {
    length: 312.6, width: 221.2, thickness: 15.5, cornerRadius: 12, edgeRadius: 5,
    source: "Apple tech specs, M1 Pro to M4 (2021-24); radii from photographs, +-1 mm",
  },
  "macbook-pro-16": {
    length: 355.7, width: 248.1, thickness: 16.8, cornerRadius: 12, edgeRadius: 5,
    source: "Apple tech specs, M1 Pro to M4 (2021-24); radii from photographs, +-1 mm",
  },
};

/**
 * A device from `DEVICES` as a solid, centred on the origin, front edge toward
 * -Y. With `clearance` it is grown outward, radii too: the cutter a holder
 * takes out of itself. `device("macbook-pro-16", { clearance: 1 })`.
 */
export function device(name: string, options: { clearance?: number } = {}): Shape {
  const body = DEVICES[name];
  if (!body) {
    throw new Error(
      `unknown device ${JSON.stringify(name)}; DEVICES has ${Object.keys(DEVICES).join(", ")}`,
    );
  }
  const clearance = options.clearance ?? 0;
  if (!(clearance >= 0)) throw new Error("device clearance must be zero or more, in mm");
  const thickness = body.thickness + 2 * clearance;
  const edge = body.edgeRadius + clearance;
  if (2 * edge >= thickness) {
    throw new Error(
      `device ${name} is ${thickness} mm thick with this clearance, too thin for its ${edge} mm edge radius twice over`,
    );
  }
  const slab = box(body.length + 2 * clearance, body.width + 2 * clearance, thickness)
    .edges("|Z")
    .expect({ count: 4 })
    .fillet(body.cornerRadius + clearance);
  if (edge <= 0) return slab;
  return slab
    .edges(">Z")
    .expect({ count: 8 })
    .fillet(edge)
    .edges("<Z")
    .expect({ count: 8 })
    .fillet(edge);
}

/**
 * The square VESA mounting patterns, as four points for `repeat()`: MIS-D at
 * 75 and 100 mm on M4, MIS-E/F at 200 mm on M6. Centred on the origin, the
 * way a monitor arm's plate is; pick rows off the list with `filter` when a
 * bracket only reaches one of them.
 */
export function vesaPattern(size: 75 | 100 | 200): [number, number][] {
  if (size !== 75 && size !== 100 && size !== 200) {
    throw new Error(`vesaPattern takes 75, 100 or 200 (mm between holes), not ${size}`);
  }
  return grid(2, 2, size, size);
}

// ---------------------------------------------------------------------------
// Drawing in the plane. The arithmetic every placed feature otherwise repeats
// by hand: a point some distance along an edge, where two edges meet, a line
// moved sideways by a wall thickness, the convex outline through a few
// points. Pure arithmetic on numbers the script already has — a script runs
// before the kernel does, so nothing here can ask the built part where an
// edge ended up; list_entities and check_fit are the measured route for that.
// ---------------------------------------------------------------------------

/** A straight line in the XY plane through two points, with a direction. */
export class Line2d {
  constructor(
    readonly from: [number, number],
    readonly to: [number, number],
  ) {
    if (Math.hypot(to[0] - from[0], to[1] - from[1]) < 1e-9) {
      throw new Error("a line needs two distinct points");
    }
  }

  /** Unit direction from `from` toward `to`. */
  direction(): [number, number] {
    const dx = this.to[0] - this.from[0], dy = this.to[1] - this.from[1];
    const len = Math.hypot(dx, dy);
    return [dx / len, dy / len];
  }

  /** Unit normal, the direction turned a quarter turn anticlockwise: left of travel. */
  normal(): [number, number] {
    const [dx, dy] = this.direction();
    return [-dy, dx];
  }

  /** The point `distance` mm along the line from `from`; negative goes back. */
  pointAt(distance: number): [number, number] {
    const [dx, dy] = this.direction();
    return [this.from[0] + dx * distance, this.from[1] + dy * distance];
  }

  /** Length of the segment `from` to `to`. */
  length(): number {
    return Math.hypot(this.to[0] - this.from[0], this.to[1] - this.from[1]);
  }

  /**
   * The same line moved sideways by `distance`, to the left of its travel;
   * negative moves it right. A wall's inner face from its outer one, the
   * edge of a band from its centreline.
   */
  offset(distance: number): Line2d {
    const [nx, ny] = this.normal();
    return new Line2d(
      [this.from[0] + nx * distance, this.from[1] + ny * distance],
      [this.to[0] + nx * distance, this.to[1] + ny * distance],
    );
  }

  /**
   * Where this line crosses `other`, extended as far as needed in both
   * directions. Throws when they are parallel, which is the answer in that
   * case rather than a point far away.
   */
  meet(other: Line2d): [number, number] {
    const [ax, ay] = this.from, [bx, by] = this.to;
    const [cx, cy] = other.from, [dx, dy] = other.to;
    const denominator = (bx - ax) * (dy - cy) - (by - ay) * (dx - cx);
    if (Math.abs(denominator) < 1e-12) {
      throw new Error("the two lines are parallel and never meet");
    }
    const t = ((cx - ax) * (dy - cy) - (cy - ay) * (dx - cx)) / denominator;
    return [ax + t * (bx - ax), ay + t * (by - ay)];
  }

  /** The line's y at a given x, for a line that is not vertical. */
  yAt(x: number): number {
    const [dx, dy] = this.direction();
    if (Math.abs(dx) < 1e-12) throw new Error("a vertical line has no single y at an x");
    return this.from[1] + ((x - this.from[0]) / dx) * dy;
  }

  /** The line's x at a given y, for a line that is not horizontal. */
  xAt(y: number): number {
    const [dx, dy] = this.direction();
    if (Math.abs(dy) < 1e-12) throw new Error("a horizontal line has no single x at a y");
    return this.from[0] + ((y - this.from[1]) / dy) * dx;
  }
}

/** A line through two points, or from a point in a direction at an angle in degrees. */
export function line2d(from: [number, number], to: [number, number]): Line2d;
export function line2d(from: [number, number], angleDegrees: number): Line2d;
export function line2d(from: [number, number], toOrAngle: [number, number] | number): Line2d {
  if (typeof toOrAngle === "number") {
    const a = (toOrAngle * Math.PI) / 180;
    return new Line2d(from, [from[0] + Math.cos(a), from[1] + Math.sin(a)]);
  }
  return new Line2d(from, toOrAngle);
}

/**
 * A section outline stepped inward by `by` mm, as a section: the inside of a
 * wall. `outline` is any section: corners, arcs, curves, one closed `{ fit }`.
 *
 * - An arc stays an arc and a fitted curve one curve; never offset the
 *   points yourself.
 * - Where the outline turns tighter than `by`, the inset has a corner: the
 *   wall thickens into the valley.
 * - An inset that splits, vanishes or is not `by` inside is refused with the
 *   distance that fits. Inset once by the sum, never an inset of an inset.
 * - For a lofted wall use `loft`'s `wall`, not a loft of insets.
 *
 * @example
 *     // a 60 × 40 tray, 1.6 mm walls and a 2 mm floor
 *     const outline = [{ at: [-30, -20], round: 6 }, { at: [30, -20], round: 6 }, { at: [30, 20], round: 6 }, { at: [-30, 20], round: 6 }];
 *     return extrude(outline, 20).cut(extrude(inset(outline, 1.6), 20).at(0, 0, 2));
 *
 * @remarks
 * The kernel offsets the curve rather than a script offsetting points, which
 * folds in every valley narrower than the wall and hands the kernel an
 * outline that crosses itself. The loops the offset would make are removed.
 * The result is measured before it is used.
 */
export function inset(outline: SectionEntry[], by: number): SectionEntry[] {
  if (!(typeof by === "number" && Number.isFinite(by) && by > 0)) {
    throw new Error(`inset steps an outline inward by a distance in mm, more than 0; got ${JSON.stringify(by)}`);
  }
  const drawn = checkSection(outline, "an inset outline", "inset([[-10, -10], [10, -10], [10, 10], [-10, 10]], 1.6)");
  return [{ inset: drawn, by }];
}

/**
 * The outline of an involute spur gear for `extrude`: centred on the origin,
 * a tooth on +X, every flank certified to lie within `tolerance` mm of the
 * true involute.
 *
 * - `module` (mm) and `teeth` are required. Defaults: `pressureAngle` 20°,
 *   `addendum` `module`, `dedendum` `1.25 * module`.
 * - The part reports the flanks' proven bound as `curve_bound_mm`, with
 *   `curve_bound: "certified"`; `tolerance` (default 0.0001) is only a cap.
 * - `backlash` thins each tooth by that many mm at the pitch circle.
 * - Under 17 teeth at 20° is refused as undercut unless `profileShift`
 *   (x, in modules) is large enough; the refusal names the least.
 *
 * @example
 *     // module 2, 20 teeth, 10 mm thick, 6 mm bore
 *     return extrude(spurGearOutline({ module: 2, teeth: 20 }), 10).cut(cylinder(3, 12).at(0, 0, 5));
 * @example extrude(spurGearOutline({ module: 2, teeth: 12, profileShift: 0.3 }), 8)  // a 12-tooth pinion, shifted
 *
 * @remarks
 * The pitch radius is `module * teeth / 2`. Shifted gears mesh through
 * `spurGearPair`. A shift moves the tip and root out by `x · module` and thickens the tooth
 * by `2 · x · module · tan(pressureAngle)` at the pitch circle. Two gears
 * each thinned by `backlash` and centred at their centre distance are
 * `backlash · cos(pressureAngle)` apart between flanks.
 *
 * Every flank is a `{ curve }` entry on the involute of the base circle
 * (pitch radius · cos pressureAngle), certified by the Hermite remainder;
 * the tip and root are exact arcs. Below the base circle the flank runs
 * straight in along the radius. A hob leaves a thicker trochoid fillet there,
 * so the outline has no material a hobbed gear lacks and meshes wherever a
 * hobbed one does.
 *
 * Refused, with the numbers: a shift below
 * `dedendum / module − 0.25 − (teeth / 2) · sin²(pressureAngle)` (undercut:
 * 17.1 teeth unshifted at 20°, 11.2 at 25°), because this outline does not
 * draw an undercut root's trochoid; teeth pointed before the tip circle; and
 * teeth so thick the root has no room. Two unshifted gears mesh at
 * `module * (teeth1 + teeth2) / 2`, the second turned `180 / teeth2` when
 * its count is even; `spurGearPair` gives both for any pair.
 */
export function spurGearOutline(options: {
  module: number;
  teeth: number;
  pressureAngle?: number;
  profileShift?: number;
  addendum?: number;
  dedendum?: number;
  backlash?: number;
  tolerance?: number;
}): SectionEntry[] {
  const { module: m, teeth: z } = options ?? ({} as typeof options);
  checkGearModule(m, z, "spurGearOutline");
  return involuteGear({
    module: m,
    teeth: z,
    degrees: options.pressureAngle ?? 20,
    shift: options.profileShift ?? 0,
    addendum: options.addendum ?? m,
    dedendum: options.dedendum ?? 1.25 * m,
    backlash: options.backlash ?? 0,
    tolerance: options.tolerance ?? 1e-4,
    what: "spurGearOutline",
  });
}

/**
 * Two full-depth involute spur gears that mesh, and where to put them.
 *
 * - `teeth` is `[first, second]`, `profileShift` their shifts (default
 *   `[0, 0]`); `module`, `pressureAngle` and `tolerance` are as in
 *   `spurGearOutline`.
 * - `backlash` is the pair's play in mm: built as returned, the flanks are
 *   `backlash / 2` apart in `between_bodies`.
 * - Returns `{ centres, pressureAngle, turn, outlines }`: `centres` mm
 *   between the axes, `pressureAngle` the working one in degrees, `turn` the
 *   degrees to rotate the second gear before placing it at `[centres, 0]`.
 * - Undercut, interference and a contact ratio under 1 are refused.
 *
 * @example
 *     // 12 and 30 teeth, module 2, the pinion shifted to avoid undercut
 *     const pair = spurGearPair({ module: 2, teeth: [12, 30], profileShift: [0.3, 0], backlash: 0.1 });
 *     const pinion = extrude(pair.outlines[0], 8);
 *     const wheel = extrude(pair.outlines[1], 8).rotate("z", pair.turn).at(pair.centres, 0, 0);
 *     return { pinion, wheel };
 *
 * @remarks
 * Backlash is measured along the line of action with one pair of flanks
 * touching; each gear's teeth are thinned by half of it. Refusals carry the
 * numbers: a shift that lets the hob undercut either gear, a tip of one
 * reaching below where the other's involute starts, and a contact ratio
 * under 1, which would lose contact between one pair of teeth and the next.
 *
 * Unshifted, or shifted by opposite amounts, `centres` is
 * `module * (z1 + z2) / 2`. Otherwise the working pressure angle αw follows
 * `inv αw = inv α + 2 tan α (x1 + x2) / (z1 + z2)` and `centres` is
 * `module * (z1 + z2) / 2 · cos α / cos αw`. `turn` is `180 / teeth2` for an
 * even count and 0 for an odd one, so a space faces the first gear's tooth
 * on +X. A shifted pair's tips are both lowered by
 * `(x1 + x2 − (centres − module (z1 + z2) / 2) / module) · module`, the
 * standard tip shortening, so each tip clears the other's root by
 * `0.25 · module`.
 */
export function spurGearPair(options: {
  module: number;
  teeth: [number, number];
  profileShift?: [number, number];
  pressureAngle?: number;
  backlash?: number;
  tolerance?: number;
}): { centres: number; pressureAngle: number; turn: number; outlines: [SectionEntry[], SectionEntry[]] } {
  const { module: m, teeth } = options ?? ({} as typeof options);
  const shifts = options?.profileShift ?? [0, 0];
  if (!(Array.isArray(teeth) && teeth.length === 2)) {
    throw new Error(`spurGearPair's teeth is [first, second], the two tooth counts; got ${shown(teeth)}`);
  }
  if (!(Array.isArray(shifts) && shifts.length === 2 && shifts.every((x) => typeof x === "number" && Number.isFinite(x)))) {
    throw new Error(`spurGearPair's profileShift is [first, second], two shift coefficients (usually between -0.5 and 1); got ${shown(shifts)}`);
  }
  for (const z of teeth) checkGearModule(m, z, "spurGearPair");
  const degrees = options.pressureAngle ?? 20;
  checkPressureAngle(degrees, "spurGearPair");
  const backlash = options.backlash ?? 0;
  if (!(typeof backlash === "number" && backlash >= 0 && Number.isFinite(backlash))) {
    throw new Error(`spurGearPair's backlash is the pair's play in mm along the line of action, at least 0; got ${shown(backlash)}`);
  }
  const [z1, z2] = teeth;
  const [x1, x2] = shifts;
  for (const [z, x] of [
    [z1, x1],
    [z2, x2],
  ]) {
    checkUndercut(m, z, x, degrees, 1.25 * m, `spurGearPair's ${z}-tooth gear`);
  }
  const alpha = (degrees * Math.PI) / 180;
  const inv = (a: number) => Math.tan(a) - a;
  const target = inv(alpha) + (2 * Math.tan(alpha) * (x1 + x2)) / (z1 + z2);
  if (!(target > 0)) {
    const least = (-(z1 + z2) * inv(alpha)) / (2 * Math.tan(alpha));
    throw new Error(
      `spurGearPair: shifts ${x1} and ${x2} thin the teeth so far that they meet at no centre distance: x1 + x2 must be more than −(z1 + z2) · inv(${degrees}°) / (2 tan ${degrees}°) = ${least.toFixed(4)}. Shift one of the gears out`,
    );
  }
  // inv is increasing and convex on (0, π/2); Newton from above converges.
  let working = Math.max(alpha, Math.cbrt(3 * target)) + 0.1;
  for (let k = 0; k < 60; k++) {
    const step = (inv(working) - target) / Math.tan(working) ** 2;
    working -= step;
    if (Math.abs(step) < 1e-15) break;
  }
  if (!(working > 0 && working < Math.PI / 2 && Math.abs(inv(working) - target) < 1e-12)) {
    throw new Error(`spurGearPair: shifts ${x1} and ${x2} are too large for ${z1} and ${z2} teeth to find a working pressure angle; shift the gears less far out`);
  }
  const standard = (m * (z1 + z2)) / 2;
  const centres = (standard * Math.cos(alpha)) / Math.cos(working);
  const shortening = x1 + x2 - (centres - standard) / m;
  const tolerance = options.tolerance ?? 1e-4;
  const thinning = backlash / (2 * Math.cos(alpha));
  const gear = (z: number, x: number) =>
    involuteGear({
      module: m,
      teeth: z,
      degrees,
      shift: x,
      addendum: m * (1 - shortening),
      dedendum: 1.25 * m,
      backlash: thinning,
      tolerance,
      what: `spurGearPair's ${z}-tooth gear`,
    });
  const outlines: [SectionEntry[], SectionEntry[]] = [gear(z1, x1), gear(z2, x2)];

  const base = [z1, z2].map((z) => ((m * z) / 2) * Math.cos(alpha));
  const tips = [
    [z1, x1],
    [z2, x2],
  ].map(([z, x]) => (m * z) / 2 + m * (1 + x - shortening));
  const floors = [
    [z1, x1],
    [z2, x2],
  ].map(([z, x], i) => Math.max(base[i], (m * z) / 2 - m * (1.25 - x)));
  const line = centres * Math.sin(working);
  const reach = [0, 1].map((i) => Math.sqrt(tips[i] ** 2 - base[i] ** 2));
  for (const [i, j] of [
    [0, 1],
    [1, 0],
  ]) {
    // Where gear j's tip leaves gear i's flank, along the line of action.
    const along = line - reach[j];
    const radius = Math.sqrt(base[i] ** 2 + Math.max(0, along) ** 2);
    if (!(along >= 0 && radius >= floors[i] - 1e-9)) {
      throw new Error(
        `spurGearPair: the ${teeth[j]}-tooth gear's tip reaches the ${teeth[i]}-tooth gear ${along < 0 ? "below its base circle" : `at radius ${radius.toFixed(3)} mm, below its ${floors[i].toFixed(3)} mm root`} along the line of action, where its flank is no involute, so the teeth would jam. Shift the ${teeth[i]}-tooth gear out further, or the ${teeth[j]}-tooth gear in`,
      );
    }
  }
  const contactRatio = (reach[0] + reach[1] - line) / (Math.PI * m * Math.cos(alpha));
  if (!(contactRatio >= 1)) {
    throw new Error(
      `spurGearPair: the contact ratio of this pair is ${contactRatio.toFixed(3)}, under 1, so one pair of teeth lets go before the next takes up the load. Shift the gears less far apart (x1 + x2 is ${x1 + x2}), use more teeth, or a smaller pressureAngle`,
    );
  }
  return {
    centres,
    pressureAngle: (working * 180) / Math.PI,
    turn: z2 % 2 === 0 ? 180 / z2 : 0,
    outlines,
  };
}

function checkGearModule(m: unknown, z: unknown, what: string): void {
  if (!(typeof m === "number" && m > 0 && Number.isFinite(m))) {
    throw new Error(`${what} takes { module, teeth }: module is the reference diameter over the tooth count in mm, more than 0; got ${shown(m)}`);
  }
  if (!(typeof z === "number" && Number.isInteger(z) && z > 0)) {
    throw new Error(`${what}'s teeth is a whole number of teeth; got ${shown(z)}`);
  }
}

function checkPressureAngle(degrees: unknown, what: string): void {
  if (!(typeof degrees === "number" && degrees > 0 && degrees < 45)) {
    throw new Error(`${what}'s pressureAngle is in degrees, between 0 and 45 (usually 20); got ${shown(degrees)}`);
  }
}

/**
 * Refuse a gear whose hob would undercut it. The hob that cuts the root has a
 * straight flank that ends 0.25 · module short of it, in the rounded tip the
 * standard basic rack gives that clearance; past the point where the line of
 * action touches the base circle, that end cuts into the involute.
 */
function checkUndercut(m: number, z: number, x: unknown, degrees: number, dedendum: number, what: string): void {
  checkPressureAngle(degrees, what);
  if (!(typeof x === "number" && Number.isFinite(x))) {
    throw new Error(`${what}'s profileShift is the shift coefficient x, a number (usually between -0.5 and 1); got ${shown(x)}`);
  }
  const sin2 = Math.sin((degrees * Math.PI) / 180) ** 2;
  const flankDepth = dedendum / m - 0.25;
  const fewestShift = flankDepth - (z / 2) * sin2;
  if (x < fewestShift - 1e-12) {
    const depth = Math.abs(flankDepth - 1) < 1e-12 ? "1" : `${flankDepth} (dedendum / module − 0.25)`;
    const fewestTeeth = Math.ceil((2 * (flankDepth - x)) / sin2 - 1e-9);
    throw new Error(
      `${what}: a hob undercuts ${z} teeth at ${degrees}° unless the profile is shifted out by at least ${depth} − (${z} / 2) · sin²(${degrees}°) = ${fewestShift.toFixed(4)}, and profileShift is ${x}. The cut would remove working flank near the root, and this outline does not draw the trochoid an undercut root has, so it refuses rather than keep a flank the cutter removes. Use profileShift: ${(Math.ceil(fewestShift * 1000) / 1000).toFixed(3)} or more, ${fewestTeeth} or more teeth, or a larger pressureAngle`,
    );
  }
}

/**
 * One involute gear's outline. `addendum` and `dedendum` are the unshifted
 * tip and root depths.
 */
function involuteGear(g: {
  module: number;
  teeth: number;
  degrees: number;
  shift: number;
  addendum: number;
  dedendum: number;
  backlash: number;
  tolerance: number;
  what: string;
}): SectionEntry[] {
  const { module: m, teeth: z, degrees, shift: x, addendum, dedendum, backlash, tolerance, what } = g;
  if (!(addendum > 0 && dedendum > 0 && backlash >= 0)) {
    throw new Error(`${what}'s addendum and dedendum are mm beyond and inside the reference circle, more than 0, and backlash is at least 0; got ${addendum}, ${dedendum} and ${backlash}`);
  }
  checkUndercut(m, z, x, degrees, dedendum, what);
  const alpha = (degrees * Math.PI) / 180;
  const pitch = (m * z) / 2;
  const base = pitch * Math.cos(alpha);
  const tip = pitch + addendum + x * m;
  const root = pitch - dedendum + x * m;
  if (!(root > 0)) {
    throw new Error(`${what}'s root circle, ${dedendum} mm inside the ${pitch} mm reference radius and shifted ${x * m} mm out, reaches past the centre; make the dedendum smaller`);
  }
  const involute = (a: number) => Math.tan(a) - a;
  // Half the tooth's angular width at the base circle, and at radius r.
  const thickness = m * (Math.PI / 2 + 2 * x * Math.tan(alpha)) - backlash;
  const halfAtBase = thickness / (2 * pitch) + involute(alpha);
  const halfAt = (r: number) => (r <= base ? halfAtBase : halfAtBase - involute(Math.acos(base / r)));
  const rollAt = (r: number) => (r <= base ? 0 : Math.sqrt((r / base) ** 2 - 1));
  if (!(halfAt(tip) > 0)) {
    // The involute's half-width falls monotonically above the base circle.
    let low = Math.max(base, root);
    let high = tip;
    for (let k = 0; k < 60; k++) {
      const mid = (low + high) / 2;
      if (halfAt(mid) > 0) low = mid;
      else high = mid;
    }
    throw new Error(
      `${what}: the teeth come to a point at radius ${low.toFixed(3)} mm, inside the ${tip.toFixed(3)} mm tip circle (${(low - pitch).toFixed(3)} mm outside the reference circle); lower the addendum or the profileShift, or use more teeth`,
    );
  }
  const floor = Math.max(root, base);
  if (!(halfAt(floor) < Math.PI / z)) {
    throw new Error(`${what}: the tooth spaces close up at the ${floor.toFixed(3)} mm circle, so the teeth merge there: use a smaller dedendum or profileShift`);
  }
  const t0 = rollAt(root);
  const t1 = rollAt(tip);
  // |d⁴/dt⁴| of an involute of radius b is b·√(9 + t²), largest at the larger |t|.
  const fourth = (a: number, b: number) => base * Math.sqrt(9 + Math.max(a * a, b * b));
  const polar = (r: number, angle: number): [number, number] => [r * Math.cos(angle), r * Math.sin(angle)];
  const outline: SectionEntry[] = [];
  for (let k = 0; k < z; k++) {
    const centre = (2 * Math.PI * k) / z;
    const up = centre - halfAtBase;
    const down = centre + halfAtBase;
    if (root < base) outline.push(polar(root, up));
    outline.push({
      curve: (t) => [base * (Math.cos(up + t) + t * Math.sin(up + t)), base * (Math.sin(up + t) - t * Math.cos(up + t))],
      derivative: (t) => [base * t * Math.cos(up + t), base * t * Math.sin(up + t)],
      fourth,
      from: t0,
      to: t1,
      tolerance,
    });
    outline.push({ through: polar(tip, centre) });
    outline.push({
      curve: (t) => [base * (Math.cos(down - t) - t * Math.sin(down - t)), base * (Math.sin(down - t) + t * Math.cos(down - t))],
      derivative: (t) => [base * t * Math.cos(down - t), base * t * Math.sin(down - t)],
      fourth,
      from: t1,
      to: t0,
      tolerance,
    });
    if (root < base) outline.push(polar(root, down));
    outline.push({ through: polar(root, centre + Math.PI / z) });
  }
  return outline;
}

/**
 * The convex outline through a set of points, anticlockwise, ready for
 * `extrude`. What a fan, a flare or a gusset is: "the shape that joins these
 * corners", with the convexity `extrude` demands guaranteed by construction
 * rather than checked after. Points inside the hull are dropped; three
 * distinct points that are not collinear are the least it accepts.
 */
export function hull(points: [number, number][]): [number, number][] {
  const unique = points
    .map(([x, y]): [number, number] => [x, y])
    .sort((a, b) => a[0] - b[0] || a[1] - b[1])
    .filter((p, i, all) => i === 0 || Math.hypot(p[0] - all[i - 1][0], p[1] - all[i - 1][1]) > 1e-9);
  if (unique.length < 3) throw new Error("a hull needs at least three distinct points");
  const cross = (o: [number, number], a: [number, number], b: [number, number]) =>
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lower: [number, number][] = [];
  for (const p of unique) {
    while (lower.length >= 2 && cross(lower[lower.length - 2], lower[lower.length - 1], p) <= 1e-9) lower.pop();
    lower.push(p);
  }
  const upper: [number, number][] = [];
  for (const p of [...unique].reverse()) {
    while (upper.length >= 2 && cross(upper[upper.length - 2], upper[upper.length - 1], p) <= 1e-9) upper.pop();
    upper.push(p);
  }
  const outline = [...lower.slice(0, -1), ...upper.slice(0, -1)];
  if (outline.length < 3) throw new Error("the points are collinear; a hull needs an area");
  return outline;
}

/** A point in an extruded outline: `[x, y]`, in the plane the shape is drawn on. */
export type OutlinePoint = [number, number];

/**
 * A closed outline in XY (a list of `SectionEntry`), given a thickness along Z.
 * Like every primitive it is centred on the origin in Z, from `-height / 2`
 * to `+height / 2`; the outline places it in X and Y.
 *
 * - `draft` leans the walls in by that many degrees going up, and needs a
 *   convex outline of straight edges: fillet the vertical edges afterwards.
 *
 * @example extrude([[-5, -5], [5, -5], { through: [10, 0] }, [5, 5], [-5, 5], { through: [-10, 0] }], 3)  // 20 × 10 slot
 *
 * @remarks
 * The drawn counterpart of `revolve`: a plate outline, a cam blank, an
 * L-bracket.
 */
export function extrude(
  profile: SectionEntry[],
  height: number,
  options: { draft?: number } = {},
): Shape {
  const drawn = checkSection(profile, "an extrude outline", "extrude([[-5, -5], [5, -5], [5, 5], [-5, 5]], 2)");
  if (!(height > 0)) throw new Error("extrude height must be positive");
  const draft = options.draft ?? 0;
  if (Math.abs(draft) >= 90) throw new Error("draft must be between -90 and 90 degrees");
  return new Shape(() => ({ op: "extrude", profile: drawn, height, draft }), []);
}

/**
 * A regular polygon prism along Z, first corner on +X: hex stock, a square
 * drive. `size` is across the corners unless `{ across: "flats" }`, which is
 * how hex bar and spanners are sized.
 */
export function ngon(
  sides: number,
  size: number,
  height: number,
  options: { across?: "corners" | "flats"; draft?: number } = {},
): Shape {
  if (!Number.isInteger(sides) || sides < 3) {
    throw new Error(`ngon needs at least 3 whole sides; got ${sides}`);
  }
  if (!(size > 0)) throw new Error("ngon size must be positive");
  const across = options.across ?? "corners";
  // Across the flats, the polygon touches its inscribed circle: the corners
  // stand out by 1 / cos(pi / n).
  const radius =
    across === "flats" ? size / 2 / Math.cos(Math.PI / sides) : size / 2;
  // Anticlockwise, first corner on +X. A flat lands on -Y for even side counts,
  // which is how a hex nut sits on a drawing.
  const profile: OutlinePoint[] = Array.from({ length: sides }, (_, i) => {
    const a = (i / sides) * Math.PI * 2;
    return [Math.cos(a) * radius, Math.sin(a) * radius];
  });
  return extrude(profile, height, { draft: options.draft });
}

/** A point on a routed path: `[x, y, z]`. */
export type PathPoint = [number, number, number];

/**
 * A helical path for `pipe` and `sweep`: a spring, a coil, a spiral horn.
 * Axis +Z, centred on the origin: it starts at `[radius, 0, -height / 2]`.
 *
 * - Give `turns` or `height` (= `pitch * turns`), not both.
 * - `endRadius` varies the radius linearly (a conical helix); `hand: "left"`
 *   reverses the default right-handed winding.
 * - A screw thread is `threadedRod` or `threadedHole`, not a sweep.
 *
 * @example pipe({ helix: { radius: 10, pitch: 4, turns: 5 } }, 1.5)  // a spring
 *
 * @remarks
 * Right-handed turns anticlockwise seen from above as it rises, like a
 * standard thread. The kernel sweeps a curve fitted to the exact helix and
 * refuses a fit more than 0.0001 mm off; it also refuses a pitch so tight the
 * turns collide and a radius so small the section crosses the axis, naming the
 * limit. Build time is about 0.2 s a turn for a round wire.
 */
export interface HelixPath {
  helix: {
    radius: number;
    pitch: number;
    turns?: number;
    height?: number;
    endRadius?: number;
    hand?: "right" | "left";
  };
}

/**
 * A smooth path for `pipe` and `sweep` through at least three points, in
 * order: a hose, a cable, a handle. The section starts perpendicular to it at
 * the first point. A bend tighter than the section is refused, naming where:
 * spread the points there.
 *
 * @remarks
 * One exact cubic B-spline parameterised by chord length, with no curvature at
 * its ends: the same rule as `{ spline }` in a section.
 */
export interface SplinePath {
  spline: PathPoint[];
}

function splineSpine(path: SplinePath, fn: string): { x: number; y: number; z: number }[] {
  if (!Array.isArray(path.spline) || path.spline.length < 3) {
    throw new Error(`a ${fn} spline path needs at least 3 [x, y, z] points to curve through`);
  }
  if (!path.spline.every((p) => Array.isArray(p) && p.length === 3 && p.every(Number.isFinite))) {
    throw new Error(`every point of a ${fn} spline path is [x, y, z]`);
  }
  return path.spline.map(([x, y, z]) => ({ x, y, z }));
}

/** Options shared by `pipe` and `sweep`. */
export interface SweepOptions {
  /** Centreline bend radius at every corner of a path of points. */
  bend?: number;
  /**
   * The section's size at the end relative to the start, above 0: `0.2` ends
   * at a fifth. For a point, end on something small like `0.05`.
   *
   * @remarks
   * Scaled about the path, linearly in length (in turn angle on a helix, the
   * same thing unless `endRadius` narrows it). A tapered part is B-rep only.
   */
  taper?: number;
}

/** Check a `{ helix }` path and lower it to the graph's helix. */
function helixSpine(path: HelixPath, fn: string): Record<string, unknown> {
  const h = path.helix;
  if (!h || typeof h !== "object") {
    throw new Error(`${fn} takes a path of [x, y, z] points or { helix: { radius, pitch, turns } }`);
  }
  if (!(h.radius > 0) || !(h.pitch > 0)) {
    throw new Error(`a ${fn} helix needs a positive radius and pitch; got radius ${h.radius}, pitch ${h.pitch}`);
  }
  if ((h.turns === undefined) === (h.height === undefined)) {
    throw new Error(`a ${fn} helix takes turns or height (= pitch * turns), exactly one of them`);
  }
  const turns = h.turns ?? (h.height as number) / h.pitch;
  if (!(turns > 0)) throw new Error(`a ${fn} helix needs a positive number of turns; got ${turns}`);
  if (h.endRadius !== undefined && !(h.endRadius > 0)) {
    throw new Error(`a ${fn} helix endRadius must be positive; got ${h.endRadius}`);
  }
  if (h.hand !== undefined && h.hand !== "right" && h.hand !== "left") {
    throw new Error(`a ${fn} helix hand is "right" or "left"; got ${JSON.stringify(h.hand)}`);
  }
  return {
    radius: h.radius,
    pitch: h.pitch,
    turns,
    ...(h.endRadius !== undefined && h.endRadius !== h.radius ? { endRadius: h.endRadius } : {}),
    ...(h.hand === "left" ? { hand: "left" } : {}),
  };
}

function checkTaper(taper: number, fn: string): number {
  if (!(taper > 0) || !Number.isFinite(taper)) {
    throw new Error(
      `a ${fn} taper of ${taper} is not a scale: it is the section's size at the end relative to the start, and must be more than 0 — end on a small scale such as 0.05 for a point`,
    );
  }
  return taper;
}

// Vector arithmetic for `pipe`. Deliberately not exported: every export becomes
// a reserved word inside a part script, and `add`, `cross` and `unit` are names
// a part would plausibly want for itself.
const sub = (a: PathPoint, b: PathPoint): PathPoint => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a: PathPoint, b: PathPoint) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: PathPoint, b: PathPoint): PathPoint => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const norm = (a: PathPoint) => Math.hypot(a[0], a[1], a[2]);
const scale3 = (a: PathPoint, k: number): PathPoint => [a[0] * k, a[1] * k, a[2] * k];
const add = (a: PathPoint, b: PathPoint): PathPoint => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const unit = (a: PathPoint): PathPoint => scale3(a, 1 / norm(a));
const degrees = (radians: number) => (radians * 180) / Math.PI;

/** Turn a shape built along +Z so its axis lies along `axis`. */
function alignZ(shape: Shape, axis: PathPoint): Shape {
  const perpendicular = cross([0, 0, 1], axis);
  if (norm(perpendicular) < 1e-12) {
    return axis[2] > 0 ? shape : shape.rotate("x", 180);
  }
  const angle = Math.acos(Math.min(1, Math.max(-1, axis[2])));
  const [x, y, z] = unit(perpendicular);
  return shape.rotate({ x, y, z }, degrees(angle));
}

/** Where `alignZ` sends +X, which is where a swept arc starts. */
function alignedX(axis: PathPoint): PathPoint {
  const perpendicular = cross([0, 0, 1], axis);
  if (norm(perpendicular) < 1e-12) return [1, 0, 0];
  const k = unit(perpendicular);
  const angle = Math.acos(Math.min(1, Math.max(-1, axis[2])));
  // Rodrigues, applied to +X.
  const v: PathPoint = [1, 0, 0];
  return add(
    add(scale3(v, Math.cos(angle)), scale3(cross(k, v), Math.sin(angle))),
    scale3(k, dot(k, v) * (1 - Math.cos(angle))),
  );
}

/**
 * A round tube of `diameter` along a path of points, a `HelixPath` or a
 * `SplinePath`: hydraulic line, hose, wire, spring. Another profile is `sweep`.
 *
 * - `bend` is the centreline bend radius at every corner. Without it corners
 *   are filled with a ball: fine for clearance, not a makeable tube.
 * - `taper` along a path of points needs a `bend`.
 *
 * @example pipe([[0, 0, 0], [40, 0, 0], [40, 30, 0]], 6, { bend: 10 })
 *
 * @remarks
 * A routed tube is exact: runs are cylinders and bends partial tori, trimmed
 * back to their tangent points. The ball in a square corner cannot taper,
 * which is why a tapered path needs its bends.
 */
export function pipe(
  points: PathPoint[] | HelixPath | SplinePath,
  diameter: number,
  options: SweepOptions = {},
): Shape {
  if (!(diameter > 0)) throw new Error("pipe diameter must be positive");
  const r = diameter / 2;
  const bend = options.bend ?? 0;
  if (bend < 0) throw new Error("pipe bend radius must be positive");
  const taper = checkTaper(options.taper ?? 1, "pipe");
  if (!Array.isArray(points)) {
    const tapered = taper !== 1 ? { taper } : {};
    if ("spline" in points) {
      if (bend > 0) throw new Error("a spline pipe has no corners to bend; drop the bend option");
      const spline = splineSpine(points, "pipe");
      return new Shape(() => ({ op: "sweep", circle: r, spline, ...tapered }), []);
    }
    if (bend > 0) throw new Error("a helical pipe has no corners to bend; drop the bend option");
    const helix = helixSpine(points, "pipe");
    return new Shape(() => ({ op: "sweep", circle: r, helix, ...tapered }), []);
  }
  if (points.length < 2) throw new Error("a pipe needs at least 2 path points");
  if (taper !== 1) {
    return new Shape(() => ({
      op: "sweep",
      circle: r,
      path: points.map(([x, y, z]) => ({ x, y, z })),
      ...(bend > 0 ? { bend } : {}),
      taper,
    }), []);
  }

  const legs: PathPoint[] = points.map(([x, y, z]) => [x, y, z]);
  for (let i = 0; i < legs.length - 1; i++) {
    if (norm(sub(legs[i + 1], legs[i])) < 1e-9) {
      throw new Error(`pipe path points ${i} and ${i + 1} are the same point`);
    }
  }

  // Every corner is resolved first, because a bend shortens the two runs that
  // meet at it, and only then is the chain assembled in path order.
  const from: PathPoint[] = legs.map((p) => p);
  const to: PathPoint[] = legs.map((_, i) => legs[Math.min(i + 1, legs.length - 1)]);
  const corner: (Shape | undefined)[] = legs.map(() => undefined);

  for (let i = 1; i < legs.length - 1; i++) {
    const u = unit(sub(legs[i], legs[i - 1]));
    const v = unit(sub(legs[i + 1], legs[i]));
    const turn = Math.acos(Math.min(1, Math.max(-1, dot(u, v))));
    if (turn < 1e-9) continue; // collinear: no corner to fill
    if (Math.PI - turn < 1e-9) {
      throw new Error(`the pipe path doubles back on itself at point ${i}`);
    }
    if (bend === 0) {
      corner[i] = sphere(r).at(legs[i][0], legs[i][1], legs[i][2]);
      continue;
    }

    const tangent = bend * Math.tan(turn / 2);
    const before = norm(sub(legs[i], legs[i - 1]));
    const after = norm(sub(legs[i + 1], legs[i]));
    if (tangent > before - 1e-9 || tangent > after - 1e-9) {
      // The largest radius the shorter of the two runs can carry, named rather
      // than left to the reader.
      const most = (Math.min(before, after) / Math.tan(turn / 2)).toFixed(2);
      throw new Error(
        `a bend radius of ${bend} does not fit at path point ${i}: it needs ${tangent.toFixed(2)} mm of straight either side. The most this corner takes is about ${most} mm`,
      );
    }

    to[i - 1] = sub(legs[i], scale3(u, tangent));
    from[i] = add(legs[i], scale3(v, tangent));

    const axis = unit(cross(u, v));
    const centre = add(legs[i], scale3(unit(sub(v, u)), bend / Math.cos(turn / 2)));
    const start = unit(sub(to[i - 1], centre));
    // The arc starts at the aligned +X, so the torus is spun about its own axis
    // first to put that where the run leaves off.
    const x0 = alignedX(axis);
    const spin = Math.atan2(dot(cross(x0, start), axis), dot(x0, start));
    corner[i] = alignZ(
      torus(bend, r, { sweep: degrees(turn) }).rotate("z", degrees(spin)),
      axis,
    ).at(centre[0], centre[1], centre[2]);
  }

  // Run, corner, run, corner — in path order, so every piece being fused
  // touches what is already there. Fusing the corners first builds a compound
  // of solids that do not meet, and OCCT hangs or dies on the boolean that
  // finally bridges them; three bends was enough to segfault the worker.
  const parts: Shape[] = [];
  for (let i = 0; i < legs.length - 1; i++) {
    const a = from[i];
    const b = to[i];
    const d = sub(b, a);
    const len = norm(d);
    if (len > 1e-9) {
      const mid = scale3(add(a, b), 0.5);
      parts.push(alignZ(cylinder(r, len), unit(d)).at(mid[0], mid[1], mid[2]));
    }
    const joint = corner[i + 1];
    if (joint) parts.push(joint);
  }

  return union(...parts);
}

/**
 * One loft section, lying flat at height `z`: an `outline` (a list of
 * `SectionEntry` — corners, arcs, curves), or, for the first or last section
 * only, a single `point: [x, y]` the wall closes onto — the tip of a vase or
 * a spire.
 */
export interface LoftSection {
  /** Height of the plane this section lies in. */
  z: number;
  /** Corners and curve entries, anticlockwise, first corner not repeated. */
  outline?: SectionEntry[];
  /** An apex in place of an outline, first or last section only. */
  point?: [number, number];
}

/**
 * Skin a solid through two or more `LoftSection`s at rising `z`. Walls are
 * ruled (straight between sections) unless `smooth: true`.
 *
 * - Walls pair section edges by index, so every outline needs the same edge
 *   count: a straight edge, arc or curve is one, a rounded corner adds one.
 *   A circle lofted to a square is four arcs between four corners.
 * - Listing an outline rotated twists the wall; the kernel never untwists it.
 * - `wall: t` makes a shell `t` mm thick inward from `{ fit }` sections: a
 *   vase, not a loft minus a loft. `wall: { thickness, bottom: "closed" }`
 *   gives it a floor, `top: "closed"` a lid.
 * - List each `{ fit }` section's points from the same start, same way round.
 *
 * @example loft([{ z: 0, outline: [[-10, -10], [10, -10], [10, 10], [-10, 10]] }, { z: 30, point: [0, 0] }])  // a pyramid
 * @example
 *     // a vase 80 tall with a 1.6 mm wall and a floor
 *     const ring = (r) => Array.from({ length: 120 }, (_, i) => [r * Math.cos(i * Math.PI / 60), r * Math.sin(i * Math.PI / 60)]);
 *     return loft([0, 40, 80].map((z) => ({ z, outline: [{ fit: ring(30 + z / 4), tolerance: 0.01 }] })), { smooth: true, wall: { thickness: 1.6, bottom: "closed" } });
 *
 * @remarks
 * A two-section ruled loft of an outline and its inset is the solid a drafted
 * extrude builds. A ruled loft through three or more sections reports
 * `facet_sag_mm`, how far its flat facets lie from the smooth loft: banding
 * in a render; add sections where it is large, or use `smooth: true`. A
 * walled loft reports `loft_wall_mm: { min, max }`. A smooth loft is measured against the sections' bounding
 * box and refused if it bulges past. A square lofted to itself a quarter turn
 * on is a bar twisting 90° (examples/fusion360/untriangle-v3.js).
 *
 * Facet sag is measured both ways and falls with the square of the section
 * spacing: a 180 mm shade through 41 sections measured 0.14 mm and showed
 * faint lines at 768 px.
 *
 * Fitted sections over the same number of points are fitted on one shared
 * knot vector, point `i` at the same parameter in every section, so a smooth
 * loft is one low-pole surface and a ruled one a face per stretch.
 *
 * A wall is the built outside stepped inward along its own normal (a
 * sideways inset of a sloped wall is only `t · cos(slope)` thick), at any
 * lean, with the inside skinned on the outside's parameters so the two stay
 * `t` apart between sections too. A wall more than 5 % off `t` anywhere is
 * refused, naming where, as is an outline turning tighter than the wall. An
 * open end on a wall that nearly lies flat is a knife edge, because the ring
 * is cut level: close that end, or end the loft where the wall is steeper.
 */
export function loft(
  sections: LoftSection[],
  options: {
    smooth?: boolean;
    wall?: number | { thickness: number; bottom?: "open" | "closed"; top?: "open" | "closed" };
  } = {},
): Shape {
  if (!Array.isArray(sections) || sections.length < 2) {
    throw new Error("a loft needs at least 2 sections, each { z, outline }");
  }
  const allCorners = sections.every(
    (s) => Array.isArray(s?.outline) && s.outline.every((entry) => Array.isArray(entry)),
  );
  const drawn: (SectionEntry[] | undefined)[] = [];
  for (const [i, section] of sections.entries()) {
    if (!section || !Number.isFinite(section.z)) {
      throw new Error(`loft section ${i} must be { z: number, outline: [[x, y], ...] } or { z: number, point: [x, y] }`);
    }
    if ((section.outline === undefined) === (section.point === undefined)) {
      throw new Error(`loft section ${i} takes an outline or a point, exactly one: { z, outline: [[x, y], ...] } or { z, point: [x, y] }`);
    }
    if (section.point !== undefined) {
      if (!isPair(section.point)) throw new Error(`loft section ${i}'s point is [x, y]`);
      if (i !== 0 && i !== sections.length - 1) {
        throw new Error(`loft section ${i} is a point, but only the first or last section may be one`);
      }
    } else {
      drawn[i] = checkSection(section.outline as SectionEntry[], `loft section ${i}'s outline`, "[[-5, -5], [5, -5], [5, 5], [-5, 5]]");
    }
    if (i > 0 && section.z <= sections[i - 1].z) {
      throw new Error(
        `loft sections must rise strictly: section ${i} is at z = ${section.z}, below or level with section ${i - 1} at z = ${sections[i - 1].z}`,
      );
    }
    if (allCorners && section.outline!.length !== sections[0].outline!.length) {
      throw new Error(
        `loft sections must all have the same number of outline points, because walls pair vertices by index: section ${i} has ${section.outline!.length}, section 0 has ${sections[0].outline!.length}. Repeat a vertex (a collinear point is allowed) to make the counts match`,
      );
    }
  }
  const smooth = options.smooth ?? false;
  const wall = loftWall(options.wall);
  return new Shape(() => ({
    op: "loft",
    sections: sections.map(({ z, point }, i) => (point !== undefined ? { z, point } : { outline: drawn[i], z })),
    ...(smooth ? { smooth } : {}),
    ...(wall ? { wall } : {}),
  }), []);
}

function loftWall(
  wall: number | { thickness: number; bottom?: "open" | "closed"; top?: "open" | "closed" } | undefined,
): { thickness: number; bottom?: "closed"; top?: "closed" } | undefined {
  if (wall === undefined) return undefined;
  const spec = typeof wall === "number" ? { thickness: wall } : wall;
  if (!spec || typeof spec !== "object" || !(Number.isFinite(spec.thickness) && spec.thickness > 0)) {
    throw new Error(`a loft's wall is a thickness in mm, or { thickness, bottom: "open" | "closed", top: "open" | "closed" }; got ${JSON.stringify(wall)}`);
  }
  const extra = Object.keys(spec).filter((key) => !["thickness", "bottom", "top"].includes(key));
  if (extra.length > 0) {
    throw new Error(`a loft's wall takes thickness, bottom and top; ${extra.join(", ")} is not one of them`);
  }
  const out: { thickness: number; bottom?: "closed"; top?: "closed" } = { thickness: spec.thickness };
  for (const end of ["bottom", "top"] as const) {
    const value = spec[end];
    if (value === undefined || value === "open") continue;
    if (value !== "closed") {
      throw new Error(`a loft wall's ${end} is "open" (the default, a flat ring) or "closed" (a floor as thick as the wall); got ${JSON.stringify(value)}`);
    }
    out[end] = "closed";
  }
  return out;
}

/**
 * Sweep a `SectionEntry` profile along a path of points, a `HelixPath` or a
 * `SplinePath`: `pipe` with any section. A round one stays a `pipe`.
 *
 * - The profile starts perpendicular to the path, its +Y as near global +Z as
 *   the path allows; on a helix its +X points away from the axis.
 * - A path of points that turns needs `bend`, which must fit the legs and
 *   clear the profile's own extent.
 *
 * @example sweep([[-2, -1], [2, -1], [2, 1], [-2, 1]], [[0, 0, 0], [0, 0, 20], [30, 0, 20]], { bend: 8 })
 *
 * @remarks
 * A path of points is what a bender or router follows: runs joined by tangent
 * arcs. An authored section has no ball to fill a square corner, hence the
 * required `bend`. On a helix the profile keeps its attitude to the axis all
 * the way up: a square wire coil, a thread-like ridge.
 */
export function sweep(
  profile: SectionEntry[],
  path: PathPoint[] | HelixPath | SplinePath,
  options: SweepOptions = {},
): Shape {
  const drawnProfile = checkSection(profile, "a sweep profile", "[[-2, -1], [2, -1], [2, 1], [-2, 1]]");
  const bend = options.bend ?? 0;
  if (bend < 0) throw new Error("sweep bend radius must be positive");
  const taper = checkTaper(options.taper ?? 1, "sweep");
  const tapered = taper !== 1 ? { taper } : {};
  if (!Array.isArray(path)) {
    if ("spline" in path) {
      if (bend > 0) throw new Error("a spline sweep has no corners to bend; drop the bend option");
      const spline = splineSpine(path, "sweep");
      return new Shape(() => ({ op: "sweep", profile: drawnProfile, spline, ...tapered }), []);
    }
    if (bend > 0) throw new Error("a helical sweep has no corners to bend; drop the bend option");
    const helix = helixSpine(path, "sweep");
    return new Shape(() => ({ op: "sweep", profile: drawnProfile, helix, ...tapered }), []);
  }
  if (path.length < 2) {
    throw new Error("a sweep path needs at least 2 points, or { helix: { radius, pitch, turns } }, or { spline: [[x, y, z], ...] }");
  }
  return new Shape(() => ({
    op: "sweep",
    profile: drawnProfile,
    path: path.map(([x, y, z]) => ({ x, y, z })),
    ...(bend > 0 ? { bend } : {}),
    ...tapered,
  }), []);
}

/** Which pieces a `trim` keeps; the tool decides which two words apply. */
export type TrimKeep =
  /** Inside a solid tool. */
  | "inside"
  /** Outside a solid tool. */
  | "outside"
  /** The side of a plane its normal points to. */
  | "above"
  /** The side of a plane its normal points away from. */
  | "below"
  /** A surface tool's normal side. */
  | "front"
  /** The other side of a surface tool. */
  | "back";

function trimBy(
  shape: Shape,
  tool: Shape | { plane: { point: PathPoint; normal: PathPoint } },
  keep: TrimKeep | "both",
): Shape {
  if (tool instanceof Shape) {
    return new Shape(([child, cutter]) => ({ op: "trim", child, tool: cutter, keep }), [shape, tool]);
  }
  const plane = (tool as { plane?: { point?: unknown; normal?: unknown } })?.plane;
  const triple = (v: unknown) => Array.isArray(v) && v.length === 3 && v.every((c) => typeof c === "number" && Number.isFinite(c));
  if (!plane || !triple(plane.point) || !triple(plane.normal) || (plane.normal as number[]).every((c) => c === 0)) {
    throw new Error("a trim tool is a shape, or { plane: { point: [x, y, z], normal: [x, y, z] } } with a non-zero normal");
  }
  if (keep === "inside" || keep === "outside" || keep === "front" || keep === "back") {
    throw new Error(`a plane has two sides: keep "above" (where its normal points) or "below", not ${JSON.stringify(keep)}`);
  }
  const [px, py, pz] = plane.point as number[];
  const [nx, ny, nz] = plane.normal as number[];
  return new Shape(
    ([child]) => ({ op: "trim", child, plane: { point: { x: px, y: py, z: pz }, normal: { x: nx, y: ny, z: nz } }, keep }),
    [shape],
  );
}

/**
 * Options every surface-making function takes. The curve is a list of
 * `SectionEntry`, open unless `closed: true`.
 *
 * - An open curve runs from its first entry to its last, and both are
 *   corners `[x, y]`: `[[0, 0], { through: [10, 5] }, [20, 0]]`.
 * - Or it is one `{ fit }` or `{ spline }` alone, which then runs open
 *   through its points from the first to the last.
 * - A closed curve is a section, as `extrude` takes, and makes a tube open at
 *   both ends. A full circle is two arcs between two corners.
 * - `inset` is refused in an open curve.
 */
export interface CurveOptions {
  /** `true`: the curve closes back to its start. Default `false`. */
  closed?: boolean;
}

function checkCurve(curve: SectionEntry[], closed: boolean, what: string): SectionEntry[] {
  if (typeof closed !== "boolean") throw new Error(`${what}: closed is true or false; got ${JSON.stringify(closed)}`);
  return closed
    ? checkSection(curve, what, "[[-5, -5], [5, -5], [5, 5], [-5, 5]]")
    : checkSection(curve, what, "[[0, 0], { through: [10, 5] }, [20, 0]]", true);
}

function curveOptions(options: object, allowed: string[], fn: string) {
  const extra = Object.keys(options ?? {}).filter((k) => !allowed.includes(k));
  if (extra.length) throw new Error(`${fn} takes ${allowed.join(", ")}; ${extra.join(", ")} is not an option`);
}

/**
 * A surface: a curve in the XY plane extruded `height` mm along Z, centred
 * on z = 0. Fusion's surface Extrude.
 *
 * - The curve is open unless `closed: true` (see `CurveOptions`).
 * - The normal is to the right of the curve's travel, seen from +Z.
 * - A surface has area and free edges, no volume; `.thicken(t)` makes it a
 *   solid.
 *
 * @example
 *     // an L-shaped sheet, 30 + 20 long and 40 tall: 2000 mm²
 *     return surfaceExtrude([[0, 0], [30, 0], [30, 20]], 40);
 * @example surfaceExtrude([[0, 0], { spline: [[10, 6], [20, -6]] }, [30, 0]], 40)  // a wavy sheet
 */
export function surfaceExtrude(curve: SectionEntry[], height: number, options: CurveOptions = {}): Shape {
  curveOptions(options, ["closed"], "surfaceExtrude");
  const closed = options.closed ?? false;
  const drawn = checkCurve(curve, closed, "surfaceExtrude curve");
  if (!(typeof height === "number" && Number.isFinite(height) && height > 0)) {
    throw new Error(`surfaceExtrude height is a positive length in mm; got ${JSON.stringify(height)}`);
  }
  return new Shape(() => ({ op: "surface_extrude", curve: drawn, ...(closed ? { closed } : {}), height }), []);
}

/**
 * A surface: a curve in the (radius, z) plane revolved about Z. Fusion's
 * surface Revolve: a dome from a quarter arc, a shade from a line.
 *
 * - Points are `[radius, z]`, radius >= 0. The curve is open unless
 *   `closed: true` (see `CurveOptions`).
 * - `degrees` (default 360) revolves part of a turn, anticlockwise from +X.
 * - The normal is to the right of the curve's travel: a curve drawn upward
 *   faces outward.
 *
 * @example
 *     // a radius-40 dome, open at its rim: 2π · 40² = 10,053 mm²
 *     return surfaceRevolve([[40, 0], { through: [40 * Math.SQRT1_2, 40 * Math.SQRT1_2] }, [0, 40]]);
 */
export function surfaceRevolve(curve: SectionEntry[], options: CurveOptions & { degrees?: number } = {}): Shape {
  curveOptions(options, ["closed", "degrees"], "surfaceRevolve");
  const closed = options.closed ?? false;
  const degrees = options.degrees ?? 360;
  if (!(typeof degrees === "number" && degrees > 0 && degrees <= 360)) {
    throw new Error(`surfaceRevolve degrees is more than 0 and at most 360; got ${JSON.stringify(degrees)}`);
  }
  const drawn = checkCurve(curve, closed, "surfaceRevolve curve");
  if (drawn.some((entry) => Array.isArray(entry) && entry[0] < 0)) {
    throw new Error("surfaceRevolve curve radii must be >= 0; a curve that crosses the axis sweeps through itself");
  }
  return new Shape(
    () => ({ op: "surface_revolve", curve: drawn, ...(closed ? { closed } : {}), ...(degrees !== 360 ? { degrees } : {}) }),
    [],
  );
}

/** One curve of a `surfaceLoft`, lying flat at height `z`. */
export interface LoftCurve {
  /** Height of the plane the curve lies in. */
  z: number;
  /** The curve, open unless the loft is `closed` (see `CurveOptions`). */
  curve: SectionEntry[];
}

/**
 * A surface through two or more `LoftCurve`s at rising `z`: Fusion's surface
 * Loft, for a sheet, blade or shade that stays open. Ruled between curves
 * unless `smooth: true`.
 *
 * - Curves are open unless `closed: true`, which makes a tube open at both
 *   ends (see `CurveOptions`).
 * - Pieces pair by index, as in `loft`: every curve needs the same count.
 * - For curves from points, give each curve one `{ fit }` over the same
 *   number of points, listed from the same end (or, closed, the same start
 *   and the same way round). Point `i` then lies on one line of the surface.
 * - A surface has no volume; `.thicken(t)` makes it a printable solid.
 *
 * @example
 *     // a 60° strip of a radius-40 cylinder, 40 tall: 40 · π/3 · 40 = 1676 mm²
 *     const arc = (r) => Array.from({ length: 12 }, (_, i) => {
 *       const a = (Math.PI / 3) * (i / 11) - Math.PI / 6;
 *       return [r * Math.cos(a), r * Math.sin(a)];
 *     });
 *     return surfaceLoft([
 *       { z: 0, curve: [{ fit: arc(40), tolerance: 0.01 }] },
 *       { z: 40, curve: [{ fit: arc(40), tolerance: 0.01 }] },
 *     ], { smooth: true });
 * @example surfaceLoft([{ z: 0, curve: [[-20, 0], [20, 0]] }, { z: 30, curve: [[-20, 0], [20, 0]] }]).thicken(2)  // 2400 mm³
 *
 * @remarks
 * Fitted curves over the same number of points are skinned on one shared
 * knot vector with one parameter per point, which keeps the surface light
 * enough to thicken; the fit's worst distance from its points is reported as
 * `deviation_mm`. A smooth loft is held to the curves' bounding box. This is
 * the path for generated sections, such as a reaction-diffusion profile
 * tweened and flared.
 */
export function surfaceLoft(sections: LoftCurve[], options: CurveOptions & { smooth?: boolean } = {}): Shape {
  curveOptions(options, ["closed", "smooth"], "surfaceLoft");
  if (!Array.isArray(sections) || sections.length < 2) {
    throw new Error("a surface loft needs at least 2 curves, each { z, curve }");
  }
  const closed = options.closed ?? false;
  const smooth = options.smooth ?? false;
  const drawn = sections.map((section, i) => {
    if (!section || !Number.isFinite(section.z) || !Array.isArray(section.curve)) {
      throw new Error(`surfaceLoft curve ${i} must be { z: number, curve: [...] }`);
    }
    const extra = Object.keys(section).filter((k) => k !== "z" && k !== "curve");
    if (extra.length) throw new Error(`surfaceLoft curve ${i} is { z, curve }; ${extra.join(", ")} is not part of it`);
    if (i > 0 && section.z <= sections[i - 1].z) {
      throw new Error(`surfaceLoft curves must rise strictly: curve ${i} is at z = ${section.z}, below or level with curve ${i - 1} at z = ${sections[i - 1].z}`);
    }
    return { curve: checkCurve(section.curve, closed, `surfaceLoft curve ${i}`), z: section.z };
  });
  return new Shape(
    () => ({ op: "surface_loft", sections: drawn, ...(closed ? { closed } : {}), ...(smooth ? { smooth } : {}) }),
    [],
  );
}

/**
 * A surface: a curve swept along a path. Fusion's surface Sweep.
 *
 * - The path is what `sweep` takes: points (`bend` rounds every corner),
 *   `{ helix: { radius, pitch, turns } }` or `{ spline: [[x, y, z], ...] }`.
 * - The curve is drawn in the plane square to the path's start, as `sweep`
 *   draws its profile, and is open unless `closed: true`.
 * - The normal is to the right of the curve's travel in that plane.
 *
 * @example
 *     // a ribbon 10 wide along a path 30 + π/2 · 10 + 20 long: 657 mm²
 *     return surfaceSweep([[-5, 0], [5, 0]], [[0, 0, 0], [0, 0, 40], [30, 0, 40]], { bend: 10 });
 */
export function surfaceSweep(
  curve: SectionEntry[],
  path: PathPoint[] | HelixPath | SplinePath,
  options: CurveOptions & { bend?: number } = {},
): Shape {
  curveOptions(options, ["closed", "bend"], "surfaceSweep");
  const closed = options.closed ?? false;
  const drawn = checkCurve(curve, closed, "surfaceSweep curve");
  const bend = options.bend ?? 0;
  if (!(bend >= 0)) throw new Error("surfaceSweep bend radius must be positive");
  const head = { op: "surface_sweep", curve: drawn, ...(closed ? { closed } : {}) };
  if (!Array.isArray(path)) {
    if (bend > 0) throw new Error("a helical or spline sweep has no corners to bend; drop the bend option");
    if ("spline" in path) {
      const spline = splineSpine(path, "surfaceSweep");
      return new Shape(() => ({ ...head, spline }), []);
    }
    const helix = helixSpine(path, "surfaceSweep");
    return new Shape(() => ({ ...head, helix }), []);
  }
  if (path.length < 2) {
    throw new Error("a surfaceSweep path needs at least 2 points, or { helix: { radius, pitch, turns } }, or { spline: [[x, y, z], ...] }");
  }
  return new Shape(
    () => ({ ...head, path: path.map(([x, y, z]) => ({ x, y, z })), ...(bend > 0 ? { bend } : {}) }),
    [],
  );
}

/**
 * Surfaces sewn into one along the edges they share: Fusion's Stitch. A
 * result with no free edge left is a solid.
 *
 * - The report's `kind` says `solid` only when the shell measured closed;
 *   otherwise `kind: "surface"`, its opening in `surface.free_edge_length_mm`.
 * - Edges closer than `tolerance` (default 0.01 mm, at most 0.5) are joined.
 * - `solid: true` refuses a result that does not close, listing its free
 *   edges: the way to insist on a watertight part.
 *
 * @example
 *     // four sides and two patches: a closed 20 mm cube, 8000 mm³
 *     const sides = surfaceExtrude([[-10, -10], [10, -10], [10, 10], [-10, 10]], 20, { closed: true });
 *     const top = sides.edges({ role: "boundary", at: { z: "max" } }).patch();
 *     const bottom = sides.edges({ role: "boundary", at: { z: "min" } }).patch();
 *     return stitchSurfaces(sides, top, bottom, { solid: true });
 */
export function stitchSurfaces(...args: (Shape | { tolerance?: number; solid?: boolean })[]): Shape {
  const shapes: Shape[] = [];
  let options: { tolerance?: number; solid?: boolean } = {};
  for (const a of args) {
    if (a instanceof Shape) shapes.push(a);
    else if (a && typeof a === "object") options = { ...options, ...a };
    else throw new Error(`stitchSurfaces takes surfaces and one { tolerance, solid }; got ${JSON.stringify(a)}`);
  }
  curveOptions(options, ["tolerance", "solid"], "stitchSurfaces");
  if (shapes.length === 0) throw new Error("stitchSurfaces needs at least one surface");
  const tolerance = options.tolerance ?? 0.01;
  if (!(typeof tolerance === "number" && tolerance > 0 && tolerance <= 0.5)) {
    throw new Error(`stitchSurfaces tolerance is the widest gap sewn shut, more than 0 and at most 0.5 mm; got ${JSON.stringify(tolerance)}`);
  }
  const solid = options.solid ?? false;
  return new Shape((children) => ({ op: "stitch", children, tolerance, ...(solid ? { solid } : {}) }), shapes);
}

/**
 * Fuse every shape given into one solid.
 *
 * `union(a, b, { blend: 3 })` rounds the seam the join creates by 3 mm, which
 * is a fillet in the exact backend. Two solids that meet *exactly* on a face
 * abort the kernel when the union is blended, at any radius — overlap them
 * instead, and see the `gotchas` document before reaching for a blend.
 */
export function union(...args: (Shape | BoolOptions)[]): Shape {
  const { shapes, opts } = split(args);
  return new Shape(
    (children) => ({ op: "union", children, blend: opts.blend ?? 0 }),
    shapes,
  );
}

/** Keep only the volume every one of these shapes occupies. */
export function intersect(...args: (Shape | BoolOptions)[]): Shape {
  const { shapes, opts } = split(args);
  return new Shape(
    (children) => ({ op: "intersection", children, blend: opts.blend ?? 0 }),
    shapes,
  );
}

// ---------------------------------------------------------------------------
// Helpers that exist because everyone writes them anyway
// ---------------------------------------------------------------------------

/** Copies of `shape` at every point, unioned together. */
export function repeat(shape: Shape, points: [number, number, number?][]): Shape {
  return union(...points.map(([x, y, z]) => shape.at(x, y, z ?? 0)));
}

/**
 * A centred `cols` x `rows` grid of points. `dx` and `dy` are the
 * centre-to-centre pitch, not the span: `grid(3, 1, 20, 0)` is x = -20, 0, 20.
 *
 * @remarks
 * In a 2 x 2 pattern pitch and span are the same number, which is how reading
 * it as span builds a watertight part of the wrong size.
 */
export function grid(
  cols: number,
  rows: number,
  dx: number,
  dy: number,
): [number, number][] {
  const out: [number, number][] = [];
  for (let i = 0; i < cols; i++) {
    for (let j = 0; j < rows; j++) {
      out.push([(i - (cols - 1) / 2) * dx, (j - (rows - 1) / 2) * dy]);
    }
  }
  return out;
}

/** How a bolt circle is placed relative to the centrelines. */
export interface PolarOptions {
  /** Angle of the first point, in degrees anticlockwise from +X. Default 0. */
  start?: number;
  /**
   * Turn the circle half a step so no point sits on a centreline, as flange
   * standards (ASME B16.5) place bolt holes.
   */
  straddle?: boolean;
}

/**
 * `count` points spaced evenly around a circle of `radius`, as `[x, y]` for
 * `repeat`: a bolt circle,
 * `repeat(holeFor("M6", 8, { through: true }), polar(6, 40))`.
 */
export function polar(
  count: number,
  radius: number,
  options: PolarOptions = {},
): [number, number][] {
  if (!Number.isInteger(count) || count <= 0) {
    throw new Error("polar count must be a positive integer, e.g. polar(4, 60)");
  }
  if (!Number.isFinite(radius)) {
    throw new Error("polar radius must be a number in millimetres");
  }
  const step = 360 / count;
  const start = (options.start ?? 0) + (options.straddle ? step / 2 : 0);
  return Array.from({ length: count }, (_, i) => {
    const radians = ((start + i * step) * Math.PI) / 180;
    return [Math.cos(radians) * radius, Math.sin(radians) * radius];
  });
}

/**
 * `count` copies of `shape` spun evenly about an axis through the origin and
 * unioned: for a feature that must turn with its position, like a flute round
 * a knob. Place the shape once at its radius, then `around(flute, 12)`.
 */
export function around(
  shape: Shape,
  count: number,
  axis: "x" | "y" | "z" = "z",
): Shape {
  if (!Number.isInteger(count) || count <= 0) {
    throw new Error("around count must be a positive integer, e.g. around(slot, 4)");
  }
  // The first copy is the shape itself, not a rotation by zero: an identity
  // transform in the graph is a node the kernel still has to evaluate, and it
  // makes the result differ from the same pattern written out by hand.
  return union(
    ...Array.from({ length: count }, (_, i) =>
      i === 0 ? shape : shape.rotate(axis, (i * 360) / count),
    ),
  );
}

// ---------------------------------------------------------------------------
// Generative work. A part that runs a simulation to produce its sections does
// real computation before the kernel sees anything. The sandbox that runs a
// model's script counts that work rather than timing it, so a part passes or
// fails identically on any machine; `globalThis.__parcadNative` is how the
// sandbox is reached, and where it is absent (the editor, `tools/run.ts`) the
// same answer is computed in JavaScript.
// ---------------------------------------------------------------------------

type ParcadNative = {
  scriptBudget?: (multiple: number) => void;
  reactionDiffusion?: (model: string, a: number[], b: number[], numbers: number[]) => [number[], number[]];
  outlineCrossings?: (points: [number, number][]) => [number, number][];
  outlineGaps?: (points: [number, number][], ignoreWithin: number, upTo: number) => number[];
};

const parcadNative = (): ParcadNative =>
  ((globalThis as { __parcadNative?: ParcadNative }).__parcadNative ?? {});

/** The most `scriptBudget` accepts; `MAX_WORK_MULTIPLE` in script.rs. */
const MAX_SCRIPT_BUDGET = 10;

/**
 * Let this script do `multiple` times the default work: the first line of a
 * part that runs a simulation, a growth or a search.
 *
 * - `multiple` is a whole number from 1 to 10; calling again only raises it.
 * - Work is counted in interpreter steps, not timed: the default is 600
 *   million, far more than a drawn part uses. A refusal says how much was
 *   allowed. `evaluate_part`'s `timeout_s` does not raise it.
 *
 * @example
 *     scriptBudget(8);
 *     return box(10, 10, 10);
 *
 * @remarks
 * Steps are function calls plus loop iterations, so a part that builds once
 * builds on every machine however busy. Where no budget applies (the
 * editor's own preview) it does nothing.
 */
export function scriptBudget(multiple: number): void {
  if (!(Number.isInteger(multiple) && multiple >= 1 && multiple <= MAX_SCRIPT_BUDGET)) {
    throw new Error(
      `scriptBudget takes a whole multiple of the default work from 1 to ${MAX_SCRIPT_BUDGET}, ` +
        `e.g. scriptBudget(4); got ${JSON.stringify(multiple)}`,
    );
  }
  parcadNative().scriptBudget?.(multiple);
}

// Each helper below runs natively in the sandbox and as the JavaScript beside
// it everywhere else. The two do the same float operations in the same order
// (`crates/parcad-host/src/generative.rs`), so both give the same bits.

/** A two-species reaction-diffusion run: see {@link simulateReactionDiffusion}. */
export interface ReactionDiffusionOptions {
  /**
   * `"gierer-meinhardt"`: `a` is the activator and `b` the inhibitor,
   * `a' = Da∇²a + rho·a²/(b·(1 + kappa·a²)) − decay[0]·a + source[0]` and
   * `b' = Db∇²b + rho·a² − decay[1]·b + source[1]`.
   * `"gray-scott"`: `a` is the substrate and `b` the reactant,
   * `a' = Da∇²a − a·b² + feed·(1 − a)` and `b' = Db∇²b + a·b² − (feed + kill)·b`.
   */
  model: "gierer-meinhardt" | "gray-scott";
  /** `n` cells around a ring, or `[width, height]` cells on a grid that wraps both ways, stored row by row. */
  size: number | [number, number];
  /** Starting values, one per cell. Seed the noise with the script's own seeded generator. */
  a: number[];
  b: number[];
  /** `[Da, Db]`, in cells² per unit time. */
  diffusion: [number, number];
  /** The explicit time step; refused when it is too long for the diffusion to stay stable. */
  dt: number;
  steps: number;
  /** Gierer–Meinhardt only: production, default 1. */
  rho?: number;
  /** Gierer–Meinhardt only: activator saturation, default 0; larger widens the peaks. */
  kappa?: number;
  /** Gierer–Meinhardt only: `[decay of a, decay of b]`, default `[1, 1]`. */
  decay?: [number, number];
  /** Gierer–Meinhardt only: `[basal production of a, of b]`, default `[0, 0]`. */
  source?: [number, number];
  /** Gray–Scott only: the feed rate. */
  feed?: number;
  /** Gray–Scott only: the kill rate. */
  kill?: number;
}

/**
 * Run a reaction-diffusion field to the pattern it settles into (Turing
 * spots, stripes, lobes) and return both fields, `{ a, b }`, one value per
 * cell: what a generative part grows its outline from.
 *
 * - `size: n` is a ring of cells, `size: [w, h]` a grid that wraps both
 *   ways; see `ReactionDiffusionOptions`.
 * - A `dt` too long to stay stable is refused naming the longest that is; a
 *   run that diverges is refused too.
 * - Seed the starting noise deterministically, never with `Math.random`.
 *
 * @example
 *     const cells = 100;
 *     const noise = Array.from({ length: cells }, (_, i) => 1 + 0.01 * Math.sin(i * 7.3));
 *     const { a } = simulateReactionDiffusion({
 *       model: "gierer-meinhardt", size: cells, a: noise, b: noise.map(() => 1),
 *       diffusion: [0.3, 60], dt: 0.2 / 60, steps: 12000,
 *       kappa: 0.05, decay: [1, 1.2], source: [0.01, 0],
 *     });
 *     // a ring whose radius swells where the activator peaks
 *     const ring = a.map((v, i) => [(20 + v) * Math.cos((2 * Math.PI * i) / cells), (20 + v) * Math.sin((2 * Math.PI * i) / cells)]);
 *     return extrude([{ fit: ring, tolerance: 0.05 }], 5);
 *
 * @remarks
 * Explicit Euler steps. In the sandbox it runs natively and costs one step
 * of the script's budget per cell per step, several times less than the same
 * loop written in the script; the answer is the same to the bit.
 */
export function simulateReactionDiffusion(options: ReactionDiffusionOptions): { a: number[]; b: number[] } {
  const where = "simulateReactionDiffusion";
  const { model, size, a, b, diffusion, dt, steps } = options;
  const own: Record<string, string[]> = {
    "gierer-meinhardt": ["rho", "kappa", "decay", "source"],
    "gray-scott": ["feed", "kill"],
  };
  if (!(model in own)) {
    throw new Error(`${where}: model is "gierer-meinhardt" or "gray-scott", not ${JSON.stringify(model)}`);
  }
  const common = ["model", "size", "a", "b", "diffusion", "dt", "steps"];
  for (const key of Object.keys(options)) {
    if (!common.includes(key) && !own[model].includes(key)) {
      throw new Error(`${where}: ${key} is not an option of ${model}, which takes ${own[model].join(", ")}`);
    }
  }
  const [width, height] = typeof size === "number" ? [size, 1] : size;
  if (!(Number.isInteger(width) && Number.isInteger(height) && width >= 1 && height >= 1)) {
    throw new Error(`${where}: size is a cell count or [width, height] in whole cells; got ${JSON.stringify(size)}`);
  }
  const cells = width * height;
  for (const [name, field] of [["a", a], ["b", b]] as const) {
    if (!Array.isArray(field) || field.length !== cells || !field.every(Number.isFinite)) {
      throw new Error(`${where}: ${name} must be ${cells} finite numbers, one per cell`);
    }
  }
  const finite = (v: unknown, name: string, min = -Infinity): number => {
    if (typeof v !== "number" || !Number.isFinite(v) || v < min) {
      throw new Error(`${where}: ${name} must be a finite number${min > -Infinity ? ` of at least ${min}` : ""}; got ${JSON.stringify(v)}`);
    }
    return v;
  };
  const pair = (v: unknown, name: string, fallback?: [number, number]): [number, number] => {
    if (v === undefined && fallback) return fallback;
    if (!Array.isArray(v) || v.length !== 2) throw new Error(`${where}: ${name} is a pair [for a, for b]`);
    return [finite(v[0], `${name}[0]`), finite(v[1], `${name}[1]`)];
  };
  const [da, db] = pair(diffusion, "diffusion");
  finite(da, "diffusion[0]", 0);
  finite(db, "diffusion[1]", 0);
  if (!(finite(dt, "dt") > 0)) throw new Error(`${where}: dt must be more than 0`);
  if (!(Number.isInteger(steps) && steps >= 0)) {
    throw new Error(`${where}: steps is a whole number of steps; got ${JSON.stringify(steps)}`);
  }
  const neighbours = height === 1 ? 2 : 4;
  const fastest = Math.max(da, db);
  if (dt * fastest * neighbours > 1) {
    throw new Error(
      `${where}: dt ${dt} is too long for diffusion ${fastest} to stay stable; ` +
        `explicit steps need dt at most ${1 / (fastest * neighbours)} here — take more, shorter steps`,
    );
  }
  const kinetics =
    model === "gierer-meinhardt"
      ? [
          finite(options.rho ?? 1, "rho"),
          finite(options.kappa ?? 0, "kappa"),
          ...pair(options.decay, "decay", [1, 1]),
          ...pair(options.source, "source", [0, 0]),
        ]
      : [finite(options.feed, "feed"), finite(options.kill, "kill")];
  const numbers = [width, height, dt, steps, da, db, ...kinetics];

  const native = parcadNative().reactionDiffusion;
  const [na, nb] = native
    ? native(model, a, b, numbers)
    : reactionDiffusionInScript(model, width, height, [...a], [...b], numbers);
  if (!na.every(Number.isFinite) || !nb.every(Number.isFinite)) {
    throw new Error(
      `${where}: the run diverged — a value left the finite numbers. The reaction is too fast for dt ` +
        `${dt}; halve dt and double steps, or check that b starts above zero`,
    );
  }
  return { a: na, b: nb };
}

function reactionDiffusionInScript(
  model: string,
  width: number,
  height: number,
  a: number[],
  b: number[],
  numbers: number[],
): [number[], number[]] {
  const [, , dt, steps, da, db, p0, p1, p2, p3, p4, p5] = numbers;
  const cells = width * height;
  const na = new Array<number>(cells).fill(0);
  const nb = new Array<number>(cells).fill(0);
  for (let s = 0; s < steps; s++) {
    for (let y = 0; y < height; y++) {
      const up = (y === 0 ? height - 1 : y - 1) * width;
      const down = (y + 1 === height ? 0 : y + 1) * width;
      const row = y * width;
      for (let x = 0; x < width; x++) {
        const i = row + x;
        const l = row + (x === 0 ? width - 1 : x - 1);
        const r = row + (x + 1 === width ? 0 : x + 1);
        const ai = a[i];
        const bi = b[i];
        const la = height === 1 ? a[l] + a[r] - 2.0 * ai : a[l] + a[r] + a[up + x] + a[down + x] - 4.0 * ai;
        const lb = height === 1 ? b[l] + b[r] - 2.0 * bi : b[l] + b[r] + b[up + x] + b[down + x] - 4.0 * bi;
        if (model === "gierer-meinhardt") {
          const a2 = ai * ai;
          na[i] = ai + dt * (da * la + (p0 * a2) / (bi * (1.0 + p1 * a2)) - p2 * ai + p4);
          nb[i] = bi + dt * (db * lb + p0 * a2 - p3 * bi + p5);
        } else {
          const abb = ai * bi * bi;
          na[i] = ai + dt * (da * la - abb + p0 * (1.0 - ai));
          nb[i] = bi + dt * (db * lb + abb - (p0 + p1) * bi);
        }
      }
    }
    for (let i = 0; i < cells; i++) {
      a[i] = na[i];
      b[i] = nb[i];
    }
  }
  return [a, b];
}

function outlinePoints(points: [number, number][], where: string): [number, number][] {
  if (
    !Array.isArray(points) ||
    !points.every((p) => Array.isArray(p) && p.length === 2 && Number.isFinite(p[0]) && Number.isFinite(p[1]))
  ) {
    throw new Error(`${where}: an outline is a list of [x, y] points, closing itself`);
  }
  return points;
}

function outlineOrient(p: number[], q: number[], r: number[]): number {
  return (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
}

function outlineWithin(p: number[], q: number[], r: number[]): boolean {
  return (
    r[0] >= Math.min(p[0], q[0]) && r[0] <= Math.max(p[0], q[0]) &&
    r[1] >= Math.min(p[1], q[1]) && r[1] <= Math.max(p[1], q[1])
  );
}

function segmentsMeet(p1: number[], p2: number[], p3: number[], p4: number[]): boolean {
  const d1 = outlineOrient(p3, p4, p1);
  const d2 = outlineOrient(p3, p4, p2);
  const d3 = outlineOrient(p1, p2, p3);
  const d4 = outlineOrient(p1, p2, p4);
  if (((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))) return true;
  return (
    (d1 === 0 && outlineWithin(p3, p4, p1)) ||
    (d2 === 0 && outlineWithin(p3, p4, p2)) ||
    (d3 === 0 && outlineWithin(p1, p2, p3)) ||
    (d4 === 0 && outlineWithin(p1, p2, p4))
  );
}

/**
 * Where a closed outline of points crosses or touches itself: every pair
 * `[i, j]`, `i < j`, of segments that meet without being neighbours, sorted.
 *
 * - Segment `i` runs from point `i` to point `i + 1`, the last back to the
 *   first.
 * - Empty for a clean loop: the check a growth or offset loop makes after
 *   every move.
 *
 * @example
 *     // a bow tie: its two diagonals, segments 1 and 3, cross at [5, 5]
 *     const bow = [[0, 0], [10, 0], [0, 10], [10, 10]];
 *     const pairs = outlineCrossings(bow); // [[1, 3]]
 *     return box(10, 10, 2 * pairs.length);
 *
 * @remarks
 * In the sandbox it runs natively over a spatial grid, so a script needs no
 * grid of its own.
 */
export function outlineCrossings(points: [number, number][]): [number, number][] {
  outlinePoints(points, "outlineCrossings");
  const native = parcadNative().outlineCrossings;
  if (native) return native(points);
  const n = points.length;
  const pairs: [number, number][] = [];
  if (n < 4) return pairs;
  for (let i = 0; i < n; i++) {
    for (let j = i + 2; j < n; j++) {
      if (i === 0 && j === n - 1) continue;
      if (segmentsMeet(points[i], points[(i + 1) % n], points[j], points[(j + 1) % n])) pairs.push([i, j]);
    }
  }
  return pairs;
}

/**
 * For each point of a closed outline of points, the width of the gap it
 * faces: the distance to the nearest part of the outline that is at least
 * `ignoreWithin` mm away along the outline.
 *
 * - `ignoreWithin` leaves out the point's own stretch of curve; set it to
 *   about the narrowest gap you care about.
 * - Gaps wider than `upTo` (default: no limit), and points with nothing far
 *   enough along, come back as `Infinity`.
 *
 * @example
 *     // a U: its two arms face each other across a 4 mm slot
 *     const u = [[0, 0], [14, 0], [14, 20], [9, 20], [9, 5], [5, 5], [5, 20], [0, 20]];
 *     const narrowest = Math.min(...outlineGaps(u, { ignoreWithin: 6, upTo: 10 })); // 4, at [9, 20] and [5, 20]
 *     return extrude(u, narrowest);
 *
 * @remarks
 * A lobe growing toward its neighbour, or a slot a wall must fit into, is a
 * gap narrower than it should be. In the sandbox it runs natively over a
 * spatial grid, so a script needs no grid of its own.
 */
export function outlineGaps(
  points: [number, number][],
  options: { ignoreWithin: number; upTo?: number },
): number[] {
  outlinePoints(points, "outlineGaps");
  const { ignoreWithin, upTo = Infinity } = options ?? ({} as { ignoreWithin: number });
  if (!(typeof ignoreWithin === "number" && Number.isFinite(ignoreWithin) && ignoreWithin >= 0)) {
    throw new Error(
      `outlineGaps: ignoreWithin is the length of outline either side of a point to leave out, in mm, ` +
        `0 or more — e.g. outlineGaps(ring, { ignoreWithin: 6 }); got ${JSON.stringify(ignoreWithin)}`,
    );
  }
  if (!(typeof upTo === "number" && upTo >= 0)) {
    throw new Error(`outlineGaps: upTo is a distance in mm, 0 or more; got ${JSON.stringify(upTo)}`);
  }
  const native = parcadNative().outlineGaps;
  if (native) return native(points, ignoreWithin, upTo);
  const n = points.length;
  if (n < 3) return points.map(() => Infinity);
  const run = [0];
  for (let k = 0; k < n; k++) {
    const p = points[k];
    const q = points[(k + 1) % n];
    const dx = q[0] - p[0];
    const dy = q[1] - p[1];
    run.push(run[k] + Math.sqrt(dx * dx + dy * dy));
  }
  const total = run[n];
  return points.map((p, i) => {
    let best = Infinity;
    for (let j = 0; j < n; j++) {
      let along = 0;
      if (i !== j && i !== (j + 1) % n) {
        let ahead = run[j] - run[i];
        if (ahead < 0) ahead += total;
        let behind = run[i] - run[j + 1];
        if (behind < 0) behind += total;
        along = Math.min(ahead, behind);
      }
      if (along < ignoreWithin) continue;
      const a = points[j];
      const b = points[(j + 1) % n];
      const vx = b[0] - a[0];
      const vy = b[1] - a[1];
      const length2 = vx * vx + vy * vy;
      let t = length2 > 0 ? ((p[0] - a[0]) * vx + (p[1] - a[1]) * vy) / length2 : 0;
      t = Math.min(Math.max(t, 0), 1);
      const dx = p[0] - (a[0] + t * vx);
      const dy = p[1] - (a[1] + t * vy);
      const d = Math.sqrt(dx * dx + dy * dy);
      if (d < best) best = d;
    }
    return best > upTo ? Infinity : best;
  });
}

// ---------------------------------------------------------------------------
// Flattening
// ---------------------------------------------------------------------------

/** @internal The flattened graph `build` returns. */
export interface Doc {
  units: "mm";
  root: number;
  nodes: Record<string, unknown>[];
  /** Features the graph uses that an older host cannot read; see {@link GRAPH_FEATURES}. */
  requires?: Requirement[];
  /** The part's own checks, judged on every build; see {@link Check}. */
  checks?: Check[];
}

/** @internal A feature a graph needs, in words a host that has never heard of it can print. */
export interface Requirement {
  feature: string;
  /** The last release that cannot read it. */
  after: string;
  what: string;
}

type GraphNode = Record<string, unknown>;

function sectionEntriesOf(node: GraphNode): unknown[] {
  const lists: unknown[] = [node.profile, node.curve];
  if (Array.isArray(node.sections)) {
    for (const section of node.sections) lists.push((section as GraphNode)?.outline, (section as GraphNode)?.curve);
  }
  const entries: unknown[] = [];
  const walk = (list: unknown) => {
    if (!Array.isArray(list)) return;
    for (const entry of list) {
      entries.push(entry);
      if (entry && typeof entry === "object" && !Array.isArray(entry) && "inset" in entry) {
        walk((entry as GraphNode).inset);
      }
    }
  };
  lists.forEach(walk);
  return entries;
}

function entryHas(node: GraphNode, key: string): boolean {
  return sectionEntriesOf(node).some((e) => !!e && typeof e === "object" && !Array.isArray(e) && key in e);
}

/**
 * Every graph feature a host released before it cannot read, and how to spot
 * it in a node. A host refuses a graph that requires an id it does not know,
 * by name, instead of failing on the field or silently dropping it. Adding a
 * graph feature means a row here and an id in `FEATURES` in
 * `crates/parcad-core/src/envelope.rs`; a host test holds the two together.
 * Not exported: every export is a reserved word in a script.
 */
const GRAPH_FEATURES: (Requirement & { uses: (node: GraphNode) => boolean; doc?: (doc: Doc) => boolean })[] = [
  {
    feature: "part-checks",
    after: "0.0.9",
    what: "checks carried in the part (checks: [...] beside the bodies)",
    uses: () => false,
    doc: (doc) => (doc.checks?.length ?? 0) > 0,
  },
  {
    feature: "reference-bodies",
    after: "0.0.9",
    what: "reference bodies (.reference())",
    uses: (n) => n.op === "bodies" && Array.isArray(n.bodies) && n.bodies.some((b) => (b as GraphNode)?.reference === true),
  },
  {
    feature: "print-orientation",
    after: "0.0.9",
    what: "a body's print orientation (.printedUp())",
    uses: (n) => n.op === "bodies" && Array.isArray(n.bodies) && n.bodies.some((b) => (b as GraphNode)?.printed_up !== undefined),
  },
  {
    feature: "section-curves",
    after: "0.0.6",
    what: "sections with rounded corners, arcs or splines",
    uses: (n) =>
      sectionEntriesOf(n).some(
        (e) => !isPair(e) && !!e && typeof e === "object" && !("fit" in e) && !("inset" in e),
      ),
  },
  { feature: "fitted-sections", after: "0.0.6", what: "fitted sections ({ fit })", uses: (n) => entryHas(n, "fit") },
  { feature: "inset-sections", after: "0.0.6", what: "inset sections (inset(outline, d))", uses: (n) => entryHas(n, "inset") },
  {
    feature: "expect-range",
    after: "0.0.9",
    what: "ranged expectations (.expect({ atLeast, atMost }))",
    uses: (n) => {
      const expect = (n as { expect?: { atLeast?: number; atMost?: number } }).expect;
      return expect !== undefined && (expect.atLeast !== undefined || expect.atMost !== undefined);
    },
  },
  {
    feature: "sweep-spline",
    after: "0.0.6",
    what: "sweeps and pipes along a spline",
    uses: (n) => n.op === "sweep" && Array.isArray(n.spline) && n.spline.length > 0,
  },
  {
    feature: "loft-point",
    after: "0.0.6",
    what: "lofts that close onto a point",
    uses: (n) => n.op === "loft" && Array.isArray(n.sections) && n.sections.some((s) => (s as GraphNode)?.point != null),
  },
  { feature: "bspline-knots", after: "0.0.6", what: "B-spline sections with their own knot vector", uses: (n) => entryHas(n, "knots") },
  {
    feature: "held-curves",
    after: "0.0.6",
    what: "section curves drawn from a function, with a stated bound",
    uses: (n) => entryHas(n, "within"),
  },
  { feature: "loft-wall", after: "0.0.6", what: "lofts with a wall (loft(sections, { wall }))", uses: (n) => n.op === "loft" && n.wall != null },
  {
    feature: "surfaces",
    after: "0.0.6",
    what: "surface modelling (surfaceLoft, stitchSurfaces, trim, thicken and the rest)",
    uses: (n) =>
      SURFACE_OPS.includes(n.op as string) ||
      (!!n.selector && typeof n.selector === "object" && (n.selector as GraphNode).role === "boundary"),
  },
];

const SURFACE_OPS = [
  "surface_extrude",
  "surface_revolve",
  "surface_loft",
  "surface_sweep",
  "patch",
  "stitch",
  "trim",
  "thicken",
  "offset_surface",
];

function stamped(doc: Doc): Doc {
  const requires = GRAPH_FEATURES.filter((f) => doc.nodes.some(f.uses) || f.doc?.(doc)).map(({ feature, after, what }) => ({
    feature,
    after,
    what,
  }));
  return requires.length ? { ...doc, requires } : doc;
}

/** An axis that points up on the printer; see {@link Shape.printedUp}. */
export type PrintAxis = "+x" | "x" | "-x" | "+y" | "y" | "-y" | "+z" | "z" | "-z";

/**
 * What a script may return: one shape, or an object naming each body of a
 * part that stays in several — `return { base, lid }` — where one key,
 * `checks`, may be a list of {@link Check}s instead of a body.
 */
export type Part = Shape | Record<string, Shape | Check[]>;

/**
 * One rule the built part must hold, written as `checks: [...]` beside the
 * bodies it is about. Judged on every build from what the kernel measured,
 * and reported first: a verdict, then each failing check with its
 * measurement, where, and its `why`.
 *
 * - One head key per check: `clear`, `interferes`, `touching` (body
 *   pairs), `wall`, `size`, `standsOn`, `bodies` or `watertight`.
 * - Qualifiers: `atLeast` (mm on `clear`, mm³ on `interferes`),
 *   `deeperThan` (mm), `contactAtLeast` (mm²), `ignore` and `on` on `wall`.
 *   Names must be bodies and tags the part has.
 * - evaluate_part reports a failure and builds on; export_part and
 *   save_project refuse it without `allow_failing: "<reason>"`.
 *
 * @example
 *     const plate = box(60, 40, 3).tag("plate");
 *     const stack = cylinder(12, 20).at(0, 0, 11.7).tag("stack");
 *     return {
 *       plate, stack,
 *       checks: [
 *         { clear: ["plate", "stack"], atLeast: 0.2, why: "coins must not bind" },
 *         { wall: { min: 1 }, ignore: ["feather"] },
 *         { size: { max: [115, 65, 40] } },
 *       ],
 *     };
 *
 * @remarks
 * A catch is designed to a depth and a seat is an area, so `deeperThan` and
 * `contactAtLeast` are the qualifiers to reach for there: `atLeast` on
 * `interferes` is a shared volume, which 0.002 mm³ along a coin's rim
 * satisfies at a bite of two microns (docs/COIN_HOLDER_REVIEW.md §2.3).
 * No new global: every export is a reserved word in a script (DSL_GAPS §7),
 * so `assert`, `check` and `clear` would each break saved parts, and
 * `clearance` is already taken. The one thing lost is a body named
 * `checks`, refused in a sentence. The kernel already measures every pair
 * and the snapshot already carries size, bed contact, piece count and
 * watertightness; only `wall` costs anything, the thickness sweep at the
 * check's own threshold. docs/COIN_HOLDER_REVIEW.md B2 is why a check lives
 * in the part: a check in a throwaway script is one that stops being re-run.
 */
export interface Check {
  /** The two bodies never touch, by at least `atLeast` mm. */
  clear?: [string, string];
  /** The two bodies overlap: a catch that has to catch. */
  interferes?: [string, string];
  /** The two bodies are flush: neither gap nor overlap. */
  touching?: [string, string];
  /** Nothing in the part is thinner than `min` mm, measured as `measure_wall_thickness` does. */
  wall?: { min: number };
  /** The part fits inside `max` mm on each axis, as drawn. */
  size?: { max: [number, number, number] };
  /** At least this fraction of the footprint reaches the bed, 0 to 1. */
  standsOn?: { atLeast: number };
  /** The part is exactly this many free-standing pieces. */
  bodies?: number;
  /** The mesh closes. `true` is the only value. */
  watertight?: true;
  /** For `clear`, the least clearance, mm; for `interferes`, the least shared volume, mm³. */
  atLeast?: number;
  /** For `interferes`: how far the two must reach into each other, mm. */
  deeperThan?: number;
  /** For `touching`: the least surface the two share, mm²; a corner graze is 0. */
  contactAtLeast?: number;
  /** For `wall`: tags whose surfaces are left out, and `"feather"` or `"edge"` to leave out that kind of thin reading. */
  ignore?: string[];
  /** For `wall`: only material on these tags' surfaces counts. */
  on?: string[];
  /** Free text, echoed back when the check fails. */
  why?: string;
}

/** The keys a check may carry, and the key an author might write for one. */
const CHECK_KEYS = ["clear", "interferes", "touching", "wall", "size", "standsOn", "bodies", "watertight", "atLeast", "deeperThan", "contactAtLeast", "ignore", "on", "why"];
const CHECK_HEADS = CHECK_KEYS.slice(0, 8);
const CHECK_SYNONYMS: Record<string, string> = {
  depth: "deeperThan",
  depthmm: "deeperThan",
  deeper: "deeperThan",
  bite: "deeperThan",
  contact: "contactAtLeast",
  contactmm2: "contactAtLeast",
  clearance: "atLeast",
  clearancemm: "atLeast",
  gap: "atLeast",
  min: "atLeast",
  minimum: "atLeast",
  atmost: "atLeast",
  thickness: "wall",
  thin: "wall",
  wallthickness: "wall",
  interfere: "interferes",
  interference: "interferes",
  overlap: "interferes",
  overlaps: "interferes",
  touch: "touching",
  touches: "touching",
  envelope: "size",
  fits: "size",
  stands: "standsOn",
  footprint: "standsOn",
  bed: "standsOn",
  reason: "why",
  because: "why",
  except: "ignore",
  only: "on",
};

/**
 * Why a check cannot be read, naming what to write instead, or `undefined`
 * when it can. The shape only — which bodies exist is checked by `build`,
 * and the numbers by the host (`Check::validate` in `parcad-core`).
 */
function checkShapeError(check: unknown, index: number): string | undefined {
  const at = `check ${index + 1}`;
  if (!isObject(check)) {
    return `${at} is ${Array.isArray(check) ? "an array" : `a ${typeof check}`}, not an object such as { clear: ["top", "stacks"], atLeast: 0.2 }`;
  }
  const named = unknownKeys(check, CHECK_KEYS, (key, value) => {
    const normal = key.replace(/[_\- ]/g, "").toLowerCase();
    const own = CHECK_KEYS.find((known) => known.toLowerCase() === normal);
    if (own !== undefined) return spelled(own, value);
    const synonym = CHECK_SYNONYMS[normal];
    return synonym === undefined ? undefined : spelled(synonym, value);
  });
  if (named) return `${at} ${named}. A check's keys are ${list(CHECK_KEYS)}.`;
  const heads = CHECK_HEADS.filter((key) => check[key] !== undefined);
  if (heads.length === 0) return `${at} names nothing to check; one of ${list(CHECK_HEADS)} says what it reads.`;
  if (heads.length > 1) return `${at} carries ${list(heads)} at once; a check reads one thing, so write one check per key.`;
  for (const key of ["clear", "interferes", "touching"]) {
    const pair = check[key];
    if (pair === undefined) continue;
    if (!Array.isArray(pair) || pair.length !== 2 || !pair.every((name) => typeof name === "string")) {
      return `${at}: ${key} is a pair of body names, e.g. ${key}: ["top", "stacks"], not ${render(pair)}.`;
    }
  }
  return undefined;
}

// The runners (engine.ts, tools/run.ts, script.rs) say the same thing when a
// script returns neither; not exported, because an export is a reserved word.
const RETURN_HINT =
  "the script must return a shape, or an object of named shapes for a part in several bodies.\n" +
  "End it with something like:  return body.cut(hole)   or   return { base, lid }";

/**
 * Flatten a shape, or an object of named bodies, into the JSON graph. A
 * script does not need it: returning the part is enough.
 *
 * @remarks
 * Nodes are memoised by identity, so a shape used in several places is one
 * node with several parents, evaluated once, even across bodies.
 */
export function build(
  root: Part,
  treatments?: TreatmentSource[],
  stacks?: (string | undefined)[],
): Doc {
  const nodes: Record<string, unknown>[] = [];
  const ids = new Map<Shape, number>();

  const visit = (s: Shape): number => {
    const seen = ids.get(s);
    if (seen !== undefined) return seen;

    // Children first, so their ids exist by the time this node is emitted.
    for (const child of s.children) {
      if (child.referenceBody) {
        throw new Error(
          "a shape marked .reference() was used to build another shape; a reference is measured against " +
            "the part and never part of it, so call .reference() last, on the shape the returned object names",
        );
      }
    }
    const kids = s.children.map(visit);
    const node = s.toNode(kids);
    if (s.tagName) node.tag = s.tagName;
    if (s.materialSpec) node.material = s.materialSpec;

    const id = nodes.length;
    nodes.push(node);
    ids.set(s, id);
    if (s.treatmentCall) treatments?.push({ node: id, ...s.treatmentCall });
    if (stacks) stacks[id] = s.createdAt;
    return id;
  };

  if (root instanceof Shape) {
    if (root.printedUpDirection) {
      throw new Error(
        "printedUp names how a body prints, and a part in one shape prints as drawn; to print it another way up, return it as a named body: return { part: shape.printedUp(\"-z\") }",
      );
    }
    if (root.referenceBody) {
      throw new Error(
        "the script returned only a reference; a reference is measured against the part, so return the part " +
          "beside it: return { holder, stack: stack.reference() }",
      );
    }
    const rootId = visit(root);
    return stamped({ units: "mm", root: rootId, nodes });
  }

  if (Array.isArray(root)) {
    throw new Error(
      "the script returned an array; bodies need names, so return an object instead: " +
        "return { left, right }",
    );
  }
  if (typeof root !== "object" || root === null) throw new Error(RETURN_HINT);
  const entries = Object.entries(root).filter(([name]) => name !== "checks");
  const checks = checksOf(root);
  if (entries.length === 0) {
    throw new Error(
      checks === undefined
        ? "the script returned an empty object; return one shape, or name each body: return { base, lid }"
        : "the script returned checks and no bodies; name each body beside them: return { base, lid, checks }",
    );
  }
  const bodies = entries.map(([name, shape]) => {
    if (!(shape instanceof Shape)) {
      throw new Error(
        `body "${name}" is not a shape (it is ${describe(shape)}); every value in the returned object must be one`,
      );
    }
    if (!name.trim()) throw new Error("a body has an empty name; name each body: return { base, lid }");
    if (shape.referenceBody && shape.printedUpDirection) {
      throw new Error(
        `body "${name}" is a reference and has a print orientation; a reference is never printed, so leave .printedUp() off it`,
      );
    }
    return {
      name,
      child: visit(shape),
      ...(shape.referenceBody && { reference: true }),
      ...(shape.printedUpDirection && { printed_up: { x: shape.printedUpDirection[0], y: shape.printedUpDirection[1], z: shape.printedUpDirection[2] } }),
    };
  });
  if (bodies.every((body) => body.reference)) {
    throw new Error(
      "every body is a reference; a reference is measured against the part, so at least one body must be the " +
        "part itself — leave .reference() off that one",
    );
  }
  const rootId = nodes.length;
  nodes.push({ op: "bodies", bodies });
  if (checks !== undefined) {
    const names = bodies.map((body) => body.name);
    checks.forEach((check, index) => {
      for (const key of ["clear", "interferes", "touching"]) {
        const pair = (check as Record<string, unknown>)[key];
        if (!Array.isArray(pair)) continue;
        for (const name of pair) {
          if (!names.includes(name as string)) {
            throw new Error(
              `check ${index + 1} names a body "${name}" the returned object does not have; its bodies are ${list(names.map((n) => `"${n}"`))}`,
            );
          }
        }
      }
    });
  }
  return stamped({ units: "mm", root: rootId, nodes, ...(checks && { checks }) });
}

/** The `checks` list of a returned object, validated, or `undefined` when there is none. */
function checksOf(root: Record<string, unknown>): Check[] | undefined {
  if (!("checks" in root)) return undefined;
  const checks = root.checks;
  if (checks instanceof Shape) {
    throw new Error(
      'a body cannot be named "checks": that key holds the part\'s checks, a list of rules the build must hold (read_docs dsl, Check); rename the body',
    );
  }
  if (!Array.isArray(checks)) {
    throw new Error(
      `checks is ${describe(checks)}, not a list of checks such as [{ clear: ["top", "stacks"], atLeast: 0.2 }] (read_docs dsl, Check)`,
    );
  }
  checks.forEach((check, index) => {
    const error = checkShapeError(check, index);
    if (error !== undefined) throw new Error(error);
  });
  return checks as Check[];
}

function describe(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "an array";
  return typeof value === "object" ? "a plain object" : `a ${typeof value}`;
}

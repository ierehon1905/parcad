/**
 * The parcad modelling language.
 *
 * A script here builds a description of *intent* and nothing else — no geometry
 * is computed in JavaScript. `build()` flattens it to the JSON graph that the
 * Rust core evaluates. That split is what lets the same script outlive a change
 * of geometry kernel.
 *
 * Shapes are values. Reusing one reuses the node, so
 *
 *     const hole = cylinder(3, 40)
 *     plate.cut(hole.at(20, 0, 0), hole.at(-20, 0, 0))
 *
 * produces one cylinder with two placements, not two cylinders.
 */

import { parseEdgeSelector, parseVertexSelector } from "./selectors";

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

/**
 * A compact directional query over the logical edges of a shape.
 *
 * `>Z` means furthest in +Z, `<Y` furthest in -Y, and `|X` parallel to X.
 * Join terms with `and`: `>Z and >Y and |X` picks the top edge at positive Y
 * that runs along X. The query is resolved anew after each evaluation, rather
 * than depending on an unstable B-rep edge number.
 */
export type AxisDirection = "+x" | "-x" | "+y" | "-y" | "+z" | "-z";

/** A topology-aware alternative to the compact directional selector string. */
export interface EdgeQuery {
  /** Match edges created by this named Boolean operation. */
  generatedBy?: string;
  /** Match a curve category identified from the B-rep edge. */
  curve?: "line" | "circle";
  /** Match a circular hole rim, excluding the rim of an outside boss. */
  role?: "hole";
  /** Match an edge touching a face with this outward normal. */
  adjacentTo?: { faceNormal: AxisDirection };
  /** Match edge centres at the requested document extrema. */
  at?: Partial<Record<"x" | "y" | "z", "min" | "max">>;
}

export type EdgeSelector = string | EdgeQuery;

/** A positional query over B-rep vertices for a corner treatment. */
export interface VertexQuery {
  /** Match vertices at the requested document extrema. */
  at?: Partial<Record<"x" | "y" | "z", "min" | "max">>;
}

/** A compact vertex selector such as `>X and >Y and >Z`, or a vertex query. */
export type VertexSelector = string | VertexQuery;

/** A post-condition checked against the selected B-rep entity count. */
export interface EdgeExpectation {
  /** The exact number of selected edges or vertices the selector must match. */
  count: number;
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

/** The position of a treatment call in the editor source. */
export interface SourceLocation {
  /** One-based line in the script, not in the generated Function wrapper. */
  line: number;
  /** One-based column of the treatment method name. */
  column: number;
  method: "fillet" | "chamfer" | "smooth" | "squircle";
}

/** A selected-edge treatment node and the source call that authored it. */
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

function treatmentSource(method: SourceLocation["method"]): SourceLocation | undefined {
  if (activeTreatmentSource?.method === method) return activeTreatmentSource;
  const line = new Error().stack
    ?.split("\n")
    .map((frame) => frame.match(/parcad-editor\.js:(\d+):(\d+)/))
    .find((match): match is RegExpMatchArray => match !== null);
  if (!line) return undefined;
  return { line: Number(line[1]) - 2, column: Number(line[2]), method };
}

function assertEdgeSelector(selector: EdgeSelector) {
  if (typeof selector === "string") {
    // The full grammar, not just a non-empty check: this used to accept any
    // non-blank string and let `>Q` survive until the kernel parsed it.
    parseEdgeSelector(selector);
    return;
  }
  if (
    !selector.generatedBy &&
    !selector.curve &&
    !selector.role &&
    !selector.adjacentTo &&
    (!selector.at || !Object.values(selector.at).some(Boolean))
  ) {
    throw new Error("edge query is empty; specify generatedBy, curve, role, adjacentTo, or at");
  }
  if (selector.generatedBy !== undefined && !selector.generatedBy.trim()) {
    throw new Error("generatedBy must name a tagged operation");
  }
}

function assertVertexSelector(selector: VertexSelector) {
  if (typeof selector === "string") {
    // Previously a regex that collapsed every syntax mistake into the one
    // message about `|X`. The shared parser names the actual fault instead.
    parseVertexSelector(selector);
    return;
  }
  if (!selector.at || !Object.values(selector.at).some(Boolean)) {
    throw new Error("vertex query is empty; specify at");
  }
}

function assertEdgeExpectation(expectation: EdgeExpectation) {
  if (!Number.isInteger(expectation.count) || expectation.count <= 0) {
    throw new Error("edge expectation count must be a positive integer");
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

export class Shape {
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
   * Name this shape so selectors — and you, reading a render — can refer to it.
   *
   * Tags are the only stable way to point at part of a model. They survive any
   * change to dimensions or ordering, because they name the step that made the
   * surface rather than the surface's position in some list.
   */
  tag(name: string): Shape {
    this.name = name;
    return this;
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

  translate(x: number, y: number, z = 0): Shape {
    return new Shape(
      ([child]) => ({ op: "translate", child, by: { x, y, z } }),
      [this],
    );
  }

  /** Alias for {@link translate} that reads better when placing a feature. */
  at(x: number, y: number, z = 0): Shape {
    return this.translate(x, y, z);
  }

  /** Rotate about an axis through the origin, right-handed, in degrees. */
  rotate(axis: Vec3 | "x" | "y" | "z", degrees: number): Shape {
    const a: Vec3 =
      axis === "x"
        ? { x: 1, y: 0, z: 0 }
        : axis === "y"
          ? { x: 0, y: 1, z: 0 }
          : axis === "z"
            ? { x: 0, y: 0, z: 1 }
            : axis;
    return new Shape(
      ([child]) => ({ op: "rotate", child, axis: a, degrees }),
      [this],
    );
  }

  /**
   * Reflect in a plane through the origin, named by its normal.
   *
   * `.mirror("x")` reflects across the YZ plane — the axis names the direction
   * the shape is flipped in, not the plane it stays in. A symmetric part is
   * `union(half, half.mirror("x"))`; the reflection on its own is the left-hand
   * version of a right-hand part.
   *
   * Unlike `.scale(-1)` this is a reflection rather than a point inversion, and
   * it costs nothing in either backend: reflections are isometries, so no
   * surface changes type and the implicit field stays exact.
   */
  mirror(axis: Vec3 | "x" | "y" | "z"): Shape {
    const normal: Vec3 =
      axis === "x"
        ? { x: 1, y: 0, z: 0 }
        : axis === "y"
          ? { x: 0, y: 1, z: 0 }
          : axis === "z"
            ? { x: 0, y: 0, z: 1 }
            : axis;
    return new Shape(([child]) => ({ op: "mirror", child, normal }), [this]);
  }

  scale(x: number, y = x, z = x): Shape {
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
   * Select B-rep edges with an authored selector for a later operation.
   *
   * The selector is intentionally source-facing. The viewport's `edge@…` IDs
   * are useful for inspection during one evaluation, but are never a durable
   * script reference.
   */
  edges(selector: EdgeSelector): EdgeSelection {
    assertEdgeSelector(selector);
    return new EdgeSelection(this, selector);
  }

  /**
   * Select B-rep vertices for a corner treatment.
   *
   * `>X and >Y and >Z` means the outer corner at all three positive extrema.
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
    assertEdgeSelector(selector);
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
    assertEdgeSelector(selector);
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

  union(...rest: (Shape | BoolOptions)[]): Shape {
    return union(this, ...rest);
  }

  /** Subtract each of `tools` from this shape. */
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

export function sphere(r: number): Shape {
  return new Shape(() => ({ op: "sphere", r }), []);
}

/** A cylinder along Z with the given radius and full height. */
export function cylinder(r: number, h: number): Shape {
  return new Shape(() => ({ op: "cylinder", r, h }), []);
}

/**
 * A ring: a circle of radius `minor` swept round the Z axis at radius `major`.
 *
 * Both are radii, like {@link cylinder}'s — an O-ring is quoted by cord
 * diameter and inside diameter, so a 2 mm cord on a 20 mm ID is
 * `torus(20 / 2 + 2 / 2, 2 / 2)`, and it is worth writing the halves out.
 *
 * `minor >= major` is refused: that torus passes through its own axis, and the
 * two backends do not agree on what the resulting solid is.
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
 * A closed convex section in the (radius, z) half-plane, revolved a full turn
 * about Z.
 *
 * This is how a turned part is drawn: you author the *section*, the shape that
 * a lathe tool would leave, and the axis does the rest. It is the only
 * primitive here that is not a fixed shape with parameters, and it is what a
 * cone, a countersink, a tapered hub or a V-groove ring is made of.
 *
 * Two rules, enforced by the core rather than by this file, because a graph can
 * arrive from anywhere:
 *
 * - **radius >= 0** — a section that crosses the axis sweeps through itself.
 * - **convex** — a re-entrant section has no exact distance field, so it is
 *   refused instead of being approximated differently by each backend. Build a
 *   stepped profile as a union of convex revolves; that is how it is turned.
 */
export function revolve(profile: SectionPoint[]): Shape {
  if (profile.length < 3) {
    throw new Error(
      "a revolve section needs at least 3 [radius, z] points, e.g. revolve([[0, -5], [4, -5], [0, 5]])",
    );
  }
  if (profile.some(([r]) => r < 0)) {
    throw new Error("revolve section radii must be >= 0; mirror the section onto +radius");
  }
  return new Shape(() => ({ op: "revolve", profile }), []);
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
 * ISO metric coarse fasteners: everything a hole needs, by thread designation.
 *
 * `tap` is the drill for a coarse-pitch tapped hole; `close`/`normal`/`free`
 * are ISO 273's three clearance series; `head` is the head diameter of a socket
 * head cap screw (ISO 4762) and `csink` that of a 90° countersunk socket screw
 * (ISO 10642). Diameters in mm, always.
 *
 * Exported so a caller can see the whole table rather than discover a missing
 * size one refusal at a time — and so a size that is not here is obviously
 * absent rather than silently approximated.
 */
export const METRIC_FASTENERS: Record<
  string,
  { tap: number; close: number; normal: number; free: number; head: number; csink: number }
> = {
  M2: { tap: 1.6, close: 2.2, normal: 2.4, free: 2.6, head: 3.8, csink: 4.0 },
  M2_5: { tap: 2.05, close: 2.7, normal: 2.9, free: 3.1, head: 4.5, csink: 5.0 },
  M3: { tap: 2.5, close: 3.2, normal: 3.4, free: 3.6, head: 5.5, csink: 6.0 },
  M4: { tap: 3.3, close: 4.3, normal: 4.5, free: 4.8, head: 7.0, csink: 8.0 },
  M5: { tap: 4.2, close: 5.3, normal: 5.5, free: 5.8, head: 8.5, csink: 10.0 },
  M6: { tap: 5.0, close: 6.4, normal: 6.6, free: 7.0, head: 10.0, csink: 12.0 },
  M8: { tap: 6.8, close: 8.4, normal: 9.0, free: 10.0, head: 13.0, csink: 16.0 },
  M10: { tap: 8.5, close: 10.5, normal: 11.0, free: 12.0, head: 16.0, csink: 20.0 },
  M12: { tap: 10.2, close: 13.0, normal: 13.5, free: 14.5, head: 18.0, csink: 24.0 },
  M16: { tap: 14.0, close: 17.0, normal: 17.5, free: 18.5, head: 24.0, csink: 32.0 },
  M20: { tap: 17.5, close: 21.0, normal: 22.0, free: 24.0, head: 30.0, csink: 40.0 },
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
 * A hole cutter along Z for a named fastener, entering at z = 0 going down.
 *
 * `holeFor("M6", 12)` is a blind clearance hole 12 mm deep; `{ tapped: true }`
 * drills it for a coarse thread instead, which is how every threaded hole in
 * `examples/` is drawn — there is no thread op, and a stack of tori pretending
 * to be one is the approximation this project refuses.
 *
 * The cutter always overshoots the face it enters by 0.5 mm, and `through`
 * overshoots the far side too. That is not tidiness: a tool ending exactly on a
 * face leaves a zero-thickness sliver, and one *starting* on it can cost the rim
 * the selector was going to reach.
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

/** A point in an extruded outline: `[x, y]`, in the plane the shape is drawn on. */
export type OutlinePoint = [number, number];

/**
 * A closed convex outline in XY, given a thickness along Z.
 *
 * The counterpart of {@link revolve} for a part that is drawn rather than
 * turned: a plate outline, a cam blank, a hexagon. Like every other primitive
 * it is centred on the origin in Z, so the solid runs from `-height / 2` to
 * `+height / 2`; the outline carries its own placement in X and Y.
 *
 * Convex only, for the same reason a revolve section is, and with the same
 * escape: an L outline is `union` of two convex prisms, which is also how the
 * part would be cut.
 */
export function extrude(
  profile: OutlinePoint[],
  height: number,
  options: { draft?: number } = {},
): Shape {
  if (profile.length < 3) {
    throw new Error(
      "an extrude outline needs at least 3 [x, y] points, e.g. extrude([[-5, -5], [5, -5], [5, 5], [-5, 5]], 2)",
    );
  }
  if (!(height > 0)) throw new Error("extrude height must be positive");
  const draft = options.draft ?? 0;
  if (Math.abs(draft) >= 90) throw new Error("draft must be between -90 and 90 degrees");
  return new Shape(() => ({ op: "extrude", profile, height, draft }), []);
}

/**
 * A regular polygon prism along Z: hex stock, a square drive, a triangular key.
 *
 * `size` is measured **across the corners** by default, which is the polygon's
 * circumscribed diameter. Hex bar and every spanner in the world are specified
 * across the *flats* instead, so that is `{ across: "flats" }` rather than a
 * conversion the caller has to remember — the same reason `polar()` has a
 * `straddle` flag instead of an unexplained half-step.
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
 * A round tube of `diameter` following a path: hydraulic line, hose, wire.
 *
 * This is the honest half of what Fusion calls Sweep, and it is a bigger half
 * than it first looks. A sweep along a *spline* has no exact distance field —
 * the implicit backend would have to solve for the nearest point on the path,
 * which for a cubic is a quintic — but the two path elements a tube is actually
 * made of do: a straight run is a cylinder, and a bend is a partial torus.
 * Both are exact in both backends, so a routed tube is exact.
 *
 * `bend` is the centreline bend radius, which is how tube is specified and how
 * a bender is set. Without it the corners are square and filled with a ball of
 * the tube diameter — inside the swept envelope, fine for clearance work, and
 * not a shape anybody can make. With it, the runs are trimmed back to their
 * tangent points and an arc joins them, which is the real part.
 *
 * What is still not offered: a spline path, and a profile that is not a circle.
 */
export function pipe(
  points: PathPoint[],
  diameter: number,
  options: { bend?: number } = {},
): Shape {
  if (points.length < 2) throw new Error("a pipe needs at least 2 path points");
  if (!(diameter > 0)) throw new Error("pipe diameter must be positive");
  const r = diameter / 2;
  const bend = options.bend ?? 0;
  if (bend < 0) throw new Error("pipe bend radius must be positive");

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

/** One loft section: a convex outline lying flat at height `z`. */
export interface LoftSection {
  /** Height of the plane this section lies in. */
  z: number;
  /** `[x, y]` pairs, anticlockwise, first point not repeated. */
  outline: OutlinePoint[];
}

/**
 * Skin a solid through two or more convex outlines stacked along +Z.
 *
 * This is a B-rep-only operation, and deliberately so: a loft between
 * arbitrary outlines has no exact distance field, and an approximate one
 * would mean probes and wall-thickness checks confidently measuring a part
 * that does not exist. The implicit backend refuses a lofted part by name
 * and points here; everything that runs on the distance field — probes,
 * wall thickness, raymarched renders and sections — is unavailable for it.
 *
 * By default the walls are ruled: straight lines between consecutive
 * sections, so the surface is exactly the skin of its sections and a
 * two-section loft of an outline and its inset is the same solid a drafted
 * extrude builds. `smooth: true` fits one continuous surface through all the
 * sections instead — Fusion's default look — and the backend then measures
 * that the fit stayed inside the sections' own bounding box, refusing one
 * that bulged past it.
 *
 * Sections must be convex, for the same reason extrude and revolve sections
 * are, plus one of loft's own: the kernel pairs section vertices to build
 * the wall, and a re-entrant outline makes that pairing a silent guess. A
 * stepped or hollow loft is a boolean of convex ones.
 *
 * The pairing is by outline index, taken literally, which makes it part of
 * the intent: every section must have the same number of points, and listing
 * a section's outline rotated pairs each vertex with a different one above —
 * a *twisted* wall, authored on purpose. A square lofted to the same square
 * a quarter turn on is a bar twisting 90° over its length (see
 * examples/fusion360/untriangle-v3.js); the kernel is never allowed to
 * re-origin the sections to untwist what the outlines spell out.
 */
export function loft(
  sections: LoftSection[],
  options: { smooth?: boolean } = {},
): Shape {
  if (!Array.isArray(sections) || sections.length < 2) {
    throw new Error("a loft needs at least 2 sections, each { z, outline }");
  }
  for (const [i, section] of sections.entries()) {
    if (!section || !Number.isFinite(section.z) || !Array.isArray(section.outline)) {
      throw new Error(`loft section ${i} must be { z: number, outline: [[x, y], ...] }`);
    }
    if (i > 0 && section.z <= sections[i - 1].z) {
      throw new Error(
        `loft sections must rise strictly: section ${i} is at z = ${section.z}, below or level with section ${i - 1} at z = ${sections[i - 1].z}`,
      );
    }
    if (section.outline.length !== sections[0].outline.length) {
      throw new Error(
        `loft sections must all have the same number of outline points, because walls pair vertices by index: section ${i} has ${section.outline.length}, section 0 has ${sections[0].outline.length}. Repeat a vertex (a collinear point is allowed) to make the counts match`,
      );
    }
  }
  const smooth = options.smooth ?? false;
  return new Shape(() => ({
    op: "loft",
    sections: sections.map(({ outline, z }) => ({ outline, z })),
    ...(smooth ? { smooth } : {}),
  }), []);
}

/**
 * Sweep a convex outline along a path of straight runs joined by circular
 * bends — `pipe()` with an authored section in place of the circle.
 *
 * The path model is the one a bender or a router can follow: runs, and
 * tangent arcs of radius `bend` at every corner. `bend` is required as soon
 * as the path turns (an authored section has no ball to fill a square corner
 * with), must fit the legs either side, and must clear the profile's own
 * extent so the inner side of the bend does not sweep through itself. The
 * profile is drawn perpendicular to the first run, its +Y kept as close to
 * global +Z as that run allows.
 *
 * Like `loft` this is B-rep only: the implicit backend refuses it by name,
 * and a swept part loses the capabilities that run on the distance field.
 * A *round* section should stay a `pipe()`, which is exact in both backends.
 */
export function sweep(
  profile: OutlinePoint[],
  path: PathPoint[],
  options: { bend?: number } = {},
): Shape {
  if (!Array.isArray(path) || path.length < 2) {
    throw new Error("a sweep path needs at least 2 points");
  }
  const bend = options.bend ?? 0;
  if (bend < 0) throw new Error("sweep bend radius must be positive");
  return new Shape(() => ({
    op: "sweep",
    profile,
    path: path.map(([x, y, z]) => ({ x, y, z })),
    ...(bend > 0 ? { bend } : {}),
  }), []);
}

export function union(...args: (Shape | BoolOptions)[]): Shape {
  const { shapes, opts } = split(args);
  return new Shape(
    (children) => ({ op: "union", children, blend: opts.blend ?? 0 }),
    shapes,
  );
}

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

/** A centred `cols` x `rows` grid of points with the given spacing. */
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
   * Rotate the whole circle by half a step, so no point lands on a centreline.
   *
   * This is the convention in ASME B16.5 and most flange standards, and it is
   * worth a named option rather than an unexplained `+ 0.5` in a loop: a reader
   * can check "straddle" against a drawing, and cannot check arithmetic.
   */
  straddle?: boolean;
}

/**
 * `count` points spaced evenly around a circle of `radius`.
 *
 * The rotational counterpart to {@link grid}, and returns the same `[x, y]`
 * tuples, so it feeds {@link repeat} the same way. Every part with a bolt
 * circle used to write this loop out with `Math.cos`/`Math.sin`; the trouble
 * with that is not the length but that the standards knowledge — where the
 * holes sit relative to the centrelines — ended up encoded as arithmetic.
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
 * `count` copies of `shape`, spun evenly about an axis through the origin and
 * unioned together.
 *
 * Where {@link polar} places points, this rotates a whole shape — which is what
 * a feature that is not rotationally symmetric needs: a T-slot on each face of
 * an extrusion, a flute around a knob. Placing the shape once at its radius and
 * spinning it keeps the radius in one place instead of inside a trig call, and
 * the copies share one node in the graph.
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
// Flattening
// ---------------------------------------------------------------------------

export interface Doc {
  units: "mm";
  root: number;
  nodes: Record<string, unknown>[];
}

/**
 * Flatten a shape into the JSON graph.
 *
 * Nodes are memoised by identity, so a shape used in several places becomes one
 * node with several parents — the graph stays a DAG and the core evaluates the
 * shared work once.
 */
export function build(root: Shape, treatments?: TreatmentSource[]): Doc {
  const nodes: Record<string, unknown>[] = [];
  const ids = new Map<Shape, number>();

  const visit = (s: Shape): number => {
    const seen = ids.get(s);
    if (seen !== undefined) return seen;

    // Children first, so their ids exist by the time this node is emitted.
    const kids = s.children.map(visit);
    const node = s.toNode(kids);
    if (s.tagName) node.tag = s.tagName;

    const id = nodes.length;
    nodes.push(node);
    ids.set(s, id);
    if (s.treatmentCall) treatments?.push({ node: id, ...s.treatmentCall });
    return id;
  };

  const rootId = visit(root);
  return { units: "mm", root: rootId, nodes };
}

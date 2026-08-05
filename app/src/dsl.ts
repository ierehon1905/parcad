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

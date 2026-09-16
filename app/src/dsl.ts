/**
 * The parcad modelling language.
 *
 * A script here builds a description of *intent* and nothing else — no geometry
 * is computed in JavaScript. `build()` flattens it to the JSON graph that the
 * Rust core evaluates. That split is what lets the same script outlive a change
 * of geometry kernel.
 *
 * Every length is in millimetres, every primitive is centred on the origin and
 * placed with `.at(x, y, z)`, and a script ends by returning one shape.
 *
 * Shapes are values. Reusing one reuses the node, so
 *
 *     const hole = cylinder(3, 40)
 *     plate.cut(hole.at(20, 0, 0), hole.at(-20, 0, 0))
 *
 * produces one cylinder with two placements, not two cylinders.
 *
 * A part that is several solids — a box and its lid, a clamp in two halves,
 * a holder and the object it holds — returns an object of named shapes
 * instead of one:
 *
 *     return { base, lid }
 *
 * The bodies are built, measured and exported together and never fused. The
 * report then measures each body by name and every pair against each other
 * (`clear` by how much, or `interfering` by how many mm³), STEP writes one
 * solid per body, and STL writes them all into one file. Nothing joins or
 * constrains one body to another: each sits exactly where its script placed
 * it. Selectors, tags and treatments work inside a body, never across two.
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

/** The outward normal of a face, as `adjacentTo` spells it. */
export type AxisDirection = "+x" | "-x" | "+y" | "-y" | "+z" | "-z";

/** A topology-aware alternative to the compact directional selector string. */
export interface EdgeQuery {
  /** Match edges created by this named Boolean operation. */
  generatedBy?: string;
  /**
   * Match a curve category identified from the B-rep edge: `"line"`, `"circle"`
   * (any circular arc, including one drawn in a section), or `"spline"` — every
   * edge that is neither, which is a section's spline, Bézier or B-spline and
   * also an ellipse or intersection curve a boolean leaves.
   */
  curve?: "line" | "circle" | "spline";
  /** Match a circular hole rim, excluding the rim of an outside boss. */
  role?: "hole";
  /** Match an edge touching a face with this outward normal. */
  adjacentTo?: { faceNormal: AxisDirection };
  /** Match edge centres at the requested document extrema. */
  at?: Partial<Record<"x" | "y" | "z", "min" | "max">>;
  /**
   * How the two faces meet along the edge: `convex` is an outside corner —
   * what "break every edge" means — `concave` an inside one, and `smooth` no
   * corner at all: the boundary an earlier fillet left, or a cylinder's
   * seam. A fillet or chamfer leaves smooth edges out unless asked for them
   * by name, because there is nothing there for a rolling ball to build on.
   */
  dihedral?: "convex" | "concave" | "smooth";
  /** A straight edge parallel to this axis: the object form of `|Z`. */
  parallel?: "x" | "y" | "z";
  /** Only edges at least this long, in mm: what keeps a sliver out of a cosmetic pass. */
  longerThan?: number;
  /**
   * Only edges of one or more named features. A tag names the faces of the
   * node it is on, and those faces keep the name through every later
   * boolean, fillet, chamfer and rigid motion — so `{ on: "lip", at: { z:
   * "max" } }` is the lip's own top rim, its extremes measured among the
   * lip's edges rather than the whole part's. Lost through offset, shell and
   * intersection.
   */
  on?: string | string[];
  /** Only edges with one face from each of two features: the seam where one meets the other. */
  between?: [string, string];
}

/**
 * Either a compact directional query over a shape's logical edges, or the
 * topology-aware object form above.
 *
 * `>Z` means furthest in +Z, `<Y` furthest in -Y, and `|X` parallel to X.
 * Join terms with `and`: `>Z and >Y and |X` picks the top edge at positive Y
 * that runs along X. Either form is resolved anew after each evaluation, rather
 * than depending on an unstable B-rep edge number.
 */
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
/**
 * A surface appearance for {@link Shape.material}, in glTF's terms. Glossy is
 * low `roughness`; there is no separate gloss setting.
 *
 * @example { color: "#c9ccd1", metalness: 1, roughness: 0.35 }  // aluminium
 * @example { color: "#e8702a", roughness: 0.3, clearcoat: 1 }    // lacquered paint
 * @example { color: "#9fd4ff", opacity: 0.35, roughness: 0.1 }   // clear cover
 * @example { color: "#202020", emissive: "#30ff60" }             // a lit LED
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

/**
 * A solid, or a step on the way to one.
 *
 * Every method returns a *new* shape rather than changing this one, so a shape
 * can be placed twice, cut from two things, or kept as a tool and reused. The
 * one exception is {@link tag}, which names this shape in place: a name
 * belongs to the node, and a named copy would be a second node built twice.
 */
function fullHex(name: string, color: string | undefined): string {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color ?? "")?.[1];
  if (!hex) {
    throw new Error(`material ${name} ${JSON.stringify(color)} is not a hex colour; write it as "#c83c32" or "#c33"`);
  }
  return `#${(hex.length === 3 ? [...hex].map((c) => c + c).join("") : hex).toLowerCase()}`;
}

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
   * Name this shape so selectors — and you, reading a render — can refer to it.
   *
   * Tags are the only stable way to point at part of a model. They survive any
   * change to dimensions or ordering, because they name the step that made the
   * surface rather than the surface's position in some list.
   *
   * Unlike every other method this renames the shape itself and returns it,
   * so tagging a shape twice keeps the last name, everywhere it is used.
   */
  tag(name: string): Shape {
    this.name = name;
    return this;
  }

  /**
   * How the body this shape becomes looks in the window: a colour, and
   * optionally how rough and how metallic its surface is. Purely visual — no
   * measurement, tag or selector reads it, and an agent's render stays grey
   * unless it passes `materials: true`.
   *
   * A material colours a whole solid, never part of one. A body wears the
   * outermost material in it: put it on the shape you return, or on each
   * named body, `return { base: base.material(steel), lid }`. Shapes unioned
   * together are one solid and wear one material, the first operand's when
   * only the operands have one; a cutter's material is never used. For two
   * colours, return two bodies.
   *
   * Like {@link tag}, this changes the shape itself and returns it.
   *
   * @example plate.material({ color: "#8a9099", metalness: 0.9, roughness: 0.35 })
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
   * Rotate about an axis through the origin, in degrees.
   *
   * A positive angle is right-handed: seen from the axis's + end looking back
   * at the origin, the shape turns anticlockwise. Measured, both of them —
   * `.rotate("z", 90)` carries a feature on +X round to +Y, and
   * `.rotate("x", 90)` carries one on +Z round to -Y, which is how a cylinder
   * built along Z ends up lying along Y.
   */
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
   * The half has to be a *half*. Mirroring a body that spans the plane puts its
   * material back over the far side, so a hole cut at +x is refilled by the
   * reflected copy of the same uncut body — measured, silently, and the part
   * still builds. Mirror the features and union them onto the full body, or
   * cut both holes after the union.
   *
   * Unlike `.scale(-1)` this is a reflection rather than a point inversion, and
   * it costs nothing: reflections are isometries, so no surface changes type.
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

  /**
   * Resize about the origin. One factor scales uniformly; three stretch each
   * axis, so `sphere(10).scale(2, 1, 0.5)` is an ellipsoid of semi-axes 20, 10
   * and 5 — a figurine's body or head.
   *
   * A stretched shape's surfaces become exact B-splines: its circles are
   * ellipses, so a selector asking for `curve: "circle"` no longer finds them.
   * Fillet after stretching, not before. Every factor must be positive; a
   * reflection is `mirror`.
   */
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

  /** Fuse this shape with the others; `{ blend: r }` rounds where they meet. */
  union(...rest: (Shape | BoolOptions)[]): Shape {
    return union(this, ...rest);
  }

  /**
   * Subtract every one of `tools` from this shape, as one cut judged on its
   * result: the order they are listed in does not matter, so a bore listed
   * after the cavity it opens still opens it.
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
 * A ring: a circle of radius `minor` swept round the Z axis at radius `major`.
 *
 * Both are radii, like {@link cylinder}'s — an O-ring is quoted by cord
 * diameter and inside diameter, so a 2 mm cord on a 20 mm ID is
 * `torus(20 / 2 + 2 / 2, 2 / 2)`, and it is worth writing the halves out.
 *
 * `minor >= major` is refused: that torus passes through its own axis and
 * encloses a lens-shaped double region, which is not the groove anybody meant.
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
 * One entry of a section — the closed outline `extrude`, `revolve`, `loft` and
 * `sweep` take. A section is a list, anticlockwise, that closes back to its
 * first entry by itself (never repeat the first corner at the end):
 *
 * - `[x, y]` — a corner (`[radius, z]` in a revolve). Consecutive corners are
 *   joined by a straight edge.
 * - `{ at: [x, y], round: r }` — a corner rounded by a tangent arc of radius
 *   `r`. `at` is the *sharp* corner, where the two straight edges would meet,
 *   not where the arc starts: the round trims both edges back itself. A
 *   40 × 20 plate with 5 mm corner radii is exactly four entries,
 *   `{ at: [±20, ±10], round: 5 }`, anticlockwise. `round` only joins two
 *   straight edges.
 *
 * Between two corners, one entry says how that stretch is drawn instead of a
 * straight edge (after the last corner, it draws the closing stretch back to
 * the first):
 *
 * - `{ through: [x, y] }` — a circular arc from the corner before to the
 *   corner after, passing through this point, which must lie *on* the arc.
 *   The unambiguous way to draw an arc: a half circle between `[10, -5]` and
 *   `[10, 5]` bulging to +X is `{ through: [15, 0] }` — the chord's midpoint
 *   moved out by the radius. A full circle is two arcs between two corners.
 * - `{ radius: r }` — the shorter circular arc of radius `r` between the two
 *   corners. Positive bulges *out* of the section and negative bends *in*,
 *   whichever way round the corners are listed. `r` must be at least half the
 *   distance between the corners; exactly half is a half circle.
 *
 * A slot (stadium) `L` long overall and `w` wide, along X, has its four
 * corners where the straight sides end, at `x = ±(L − w) / 2`, `y = ±w / 2`,
 * and its half-circle ends reach `x = ±L / 2`:
 * `[[-a, -w/2], [a, -w/2], { through: [L/2, 0] }, [a, w/2], [-a, w/2], { through: [-L/2, 0] }]`
 * with `a = (L − w) / 2`.
 *
 * A section is **one** closed boundary, with no holes: a hole, slot or pocket
 * is a second shape cut out of the first (`plate.cut(extrude(slot, h))`),
 * never a second loop listed after the first.
 * - `{ spline: [[x, y], ...], start?: [dx, dy], end?: [dx, dy] }` — a smooth
 *   curve from the corner before, *through* these points, to the corner
 *   after: a cubic parameterised by chord length. `start` and `end` are the
 *   directions it leaves and arrives in; without them it has no curvature at
 *   its ends. A section that is nothing but `[{ spline: points }]` is one
 *   closed smooth curve through the points, with no corner anywhere.
 * - `{ bezier: [[x, y], ...] }` — a Bézier curve whose end points are the two
 *   corners and whose *control* points are these: one is a quadratic, two a
 *   cubic (the SVG `C` command). The curve does not pass through its control
 *   points.
 * - `{ bspline: [[x, y], ...], degree?: 3 }` — a clamped uniform B-spline
 *   with the two corners as its first and last control points and these
 *   between — the form a STEP export's poles copy into.
 * - `{ fit: [[x, y], ...], tolerance: t }` — a smooth curve *fitted* through
 *   sampled points, from the corner before to the corner after, held within
 *   `tolerance` mm of every point. This is the entry for geometry that
 *   arrives as points — a simulation, a scan, a contour, an involute sampled
 *   from its equation — where `spline` overshoots between dense points and
 *   `bspline` treats them as control points and misses them by a millimetre
 *   without saying so. The kernel fits the curve, *measures* its worst
 *   distance from the points, reports it as `deviation_mm` beside the part,
 *   and refuses when the tolerance cannot be held or the fitted curve crosses
 *   itself — naming the tolerance that would hold, or the points to thin.
 *   A section that is nothing but `[{ fit: points, tolerance }]` is one closed
 *   fitted loop with no corner. 0.01 to 0.1 mm is the usual tolerance; a
 *   tighter one costs poles, a looser one smooths the points' noise.
 *
 * A section may also be a whole outline stepped inward: `inset(outline, d)`
 * (below) builds the entry `[{ inset: outline, by: d }]` and is how a wall is
 * drawn — a shade's inner surface is its outer outline inset by the wall.
 *
 * Nothing is polygonised: arcs are exact circles and every curve is an exact
 * B-spline, so faces from arcs are cylinders, cones, tori and spheres, and an
 * edge from an arc answers `curve: "circle"` while one from a curve answers
 * `curve: "spline"`. The outline may be re-entrant (an L, a stepped shaft) but
 * must not touch or cross itself; that, an arc radius too small for its
 * corners, and a round too big for its edges are refused with the numbers
 * that would fit.
 */
export type SectionEntry =
  | [number, number]
  | { at: [number, number]; round: number }
  | { through: [number, number] }
  | { radius: number }
  | { spline: [number, number][]; start?: [number, number]; end?: [number, number] }
  | { bezier: [number, number][] }
  | { bspline: [number, number][]; degree?: number }
  | { fit: [number, number][]; tolerance: number }
  | { inset: SectionEntry[]; by: number };

const SECTION_KEYS = ["at", "round", "through", "radius", "spline", "start", "end", "bezier", "bspline", "degree", "fit", "tolerance", "inset", "by"];

function isPair(value: unknown): value is [number, number] {
  return Array.isArray(value) && value.length === 2 && value.every((n) => typeof n === "number" && Number.isFinite(n));
}

/** The highest curve degree the kernel builds: OCCT's `Geom_BSplineCurve::MaxDegree`. */
const MAX_DEGREE = 25;

/** A section that is one closed curve with no corner: `[{ spline }]` or `[{ fit, tolerance }]`. */
function lone(profile: SectionEntry[]): boolean {
  return profile.length === 1 && !Array.isArray(profile[0]) && ("spline" in (profile[0] as object) || "fit" in (profile[0] as object));
}

/**
 * Check a section's shape — the kinds of entry and their numbers — so a typo
 * reads as one here. The geometry (arcs that fit, curves that do not cross)
 * is the core's to judge, because a graph can arrive from anywhere.
 */
function checkSection(profile: SectionEntry[], what: string, example: string): SectionEntry[] {
  if (!Array.isArray(profile)) {
    throw new Error(`${what} must be a list of section entries, e.g. ${example}`);
  }
  const insetAt = profile.findIndex((entry) => entry && typeof entry === "object" && !Array.isArray(entry) && "inset" in entry);
  if (insetAt >= 0) {
    if (profile.length !== 1) {
      throw new Error(`${what} entry ${insetAt} is an inset, which is a whole section: write inset(outline, d) alone, with the corners and curves inside it`);
    }
    const e = profile[0] as { inset: SectionEntry[]; by: number };
    if (!(typeof e.by === "number" && Number.isFinite(e.by) && e.by > 0)) {
      throw new Error(`${what}: an inset steps inward by a distance in mm, more than 0; got ${JSON.stringify(e.by)}`);
    }
    checkSection(e.inset, `${what}'s inset outline`, example);
    return profile;
  }
  let corners = 0;
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
    if (unknown !== undefined) {
      throw new Error(
        `${what} entry ${i} has an unknown key "${unknown}"; a section entry is [x, y], { at, round }, { through }, { radius }, { spline }, { bezier }, { bspline } or { fit, tolerance }`,
      );
    }
    const e = entry as Record<string, unknown>;
    if ("at" in e || "round" in e) {
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
  if (corners === profile.length ? corners < 3 : corners === 0 && !lone(profile)) {
    throw new Error(
      corners === profile.length
        ? `${what} needs at least 3 corners, or corners with an arc or curve between them, e.g. ${example}`
        : `${what} has no corners; put arcs and curves between [x, y] corners, or give one { spline: points } or { fit: points, tolerance } alone for a closed smooth curve`,
    );
  }
  return profile;
}

/**
 * A closed section in the (radius, z) half-plane, revolved a full turn about Z.
 *
 * This is how a turned part is drawn: you author the *section*, the shape that
 * a lathe tool would leave, and the axis does the rest. It is what a cone, a
 * countersink, a stepped shaft, a domed cap, an O-ring gland or a V-groove
 * ring is made of. The section is a list of `SectionEntry`: corners, arcs and
 * curves — a radiused shoulder is `{ at: [r, z], round: 1 }`, a dome is an arc
 * `{ through: [...] }` from the axis round to the rim.
 *
 * The rules are enforced by the core rather than by this file, because a graph
 * can arrive from anywhere: **radius >= 0** along the whole boundary — every
 * corner, arc and curve control point, since a section that crosses the axis
 * sweeps through itself — and an outline that does not cross itself. A
 * re-entrant (stepped) section is fine.
 */
export function revolve(profile: SectionEntry[]): Shape {
  checkSection(profile, "a revolve section", "revolve([[0, -5], [4, -5], [0, 5]])");
  if (profile.some((entry) => Array.isArray(entry) && entry[0] < 0)) {
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
 * `pitch` is the ISO 261 coarse pitch; `tap` is the drill for a coarse-pitch
 * tapped hole; `close`/`normal`/`free` are ISO 273's three clearance series;
 * `head` is the head diameter of a socket head cap screw (ISO 4762) and
 * `csink` that of a 90° countersunk socket screw (ISO 10642). Millimetres,
 * always.
 *
 * Exported so a caller can see the whole table rather than discover a missing
 * size one refusal at a time — and so a size that is not here is obviously
 * absent rather than silently approximated.
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
 * A hole cutter along Z for a named fastener, entering at z = 0 going down.
 *
 * `holeFor("M6", 12)` is a blind clearance hole 12 mm deep; `{ tapped: true }`
 * drills it for a coarse thread instead — the right drawing for a hole a
 * machinist will tap. A hole whose thread is printed or must be modelled is
 * `threadedHole`.
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

/** A thread named from `METRIC_FASTENERS` ("M8", coarse pitch) or given outright. */
export type ThreadSize = string | { diameter: number; pitch: number };

/** Options shared by `threadedRod` and `threadedHole`. */
export interface ThreadOptions {
  /** `"right"` (the default, and nearly every screw) or `"left"`. */
  hand?: "right" | "left";
  /**
   * Radial allowance in mm, default 0 (the ISO basic profile exactly). A rod
   * shrinks by it and a hole grows by it, every diameter by twice the value.
   * A printed pair needs one on both parts: two parts given `c` each sit `c`
   * apart across the flanks and `2c` at crest and root, which
   * `between_bodies` reads back. 0.2 is a starting point for FDM with a
   * 0.4 mm nozzle, not a measured fit; tune it on the printer.
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
 * An external screw thread along Z, centred on the origin like `cylinder`:
 * a bolt's thread, a knob's stud, a jar's neck.
 *
 * `threadedRod("M8", 20)` is 20 mm of M8 × 1.25 with the ISO 68-1 basic
 * profile — 60° flanks, a core at the basic minor diameter (6.647 mm for M8),
 * flats of P/8 at the crest and P/4 at the root — squared off at both ends.
 * `threadedRod({ diameter: 6.35, pitch: 25.4 / 20 }, 9)` is a 1/4"-20 tripod
 * screw's thread (the same 60° basic profile). Union a head or a shank on to
 * it; it is an ordinary solid from here on.
 *
 * The tooth crosses +X at z = 0 of the rod's own frame, whatever its length,
 * so a rod and a `threadedHole` of the same size and hand mate only where
 * their frames sit a whole number of pitches apart along Z, or the hole is
 * turned about Z by 360° × offset / pitch. A bolt with a nut screwed on:
 *
 *     const bolt = threadedRod("M6", 20, { clearance: 0.2 });   // frame at z = 0
 *     const nut = box(10, 10, 5).at(0, 0, 2.5)                  // z 0 to 5
 *       .cut(threadedHole("M6", 5, { through: true, clearance: 0.2 }).at(0, 0, 5));
 *     return { bolt, nut };                                      // 5 = 5 pitches: in phase
 *
 * `between_bodies` then reads them clear by the clearance across the flanks.
 * A nut that reads **interfering** on its bolt is one of two mistakes: its
 * hole does not run all the way through where the bolt passes (the cutter
 * spans from its frame's z = +0.5 down to −depth, −depth − 0.5 with
 * `through`, so place the frame on the face it enters), or the two are out of
 * phase, which moving the nut by a whole pitch fixes. Moving it off the thread
 * fixes neither. The kernel measures every thread against its closed-form
 * volume and refuses one that reads more than 2e-5 off.
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
 *
 * `part.cut(threadedHole("M8", 10, { through: true }).at(x, y, top))` taps
 * the ISO basic profile into the part: a bore at the minor diameter and the
 * thread out to the major. It overshoots the entry face by 0.5 mm and, with
 * `through`, the far face too. The tooth crosses +X at z = 0 of the cutter's
 * frame — the entry face — so a `threadedRod` mates with it a whole number of
 * pitches away along Z. For a part a machinist will tap, draw
 * `holeFor(size, depth, { tapped: true })` instead.
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
 * A device's body as a solid, centred on the origin like every primitive, its
 * front edge toward -Y. `device("macbook-pro-16")` is the laptop;
 * `device("macbook-pro-16", { clearance: 1 })` is the cutter that leaves a
 * millimetre all round it — grown outward, its corner and edge radii grown
 * with it, which is what `.offset()` would do and what a holder cuts out of
 * itself to wrap the real thing.
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
 * The convex outline through a set of points, anticlockwise, ready for
 * `extrude`. What a fan, a flare or a gusset is: "the shape that joins these
 * corners", with the convexity `extrude` demands guaranteed by construction
 * rather than checked after. Points inside the hull are dropped; three
 * distinct points that are not collinear are the least it accepts.
 */
/**
 * A section outline stepped inward by `by` mm, as a section: the wall of a
 * hollow part. `extrude(inset(outline, 1.6), h)` is the inside of a 1.6 mm
 * wall around `extrude(outline, h)`, and a shade is the loft of outer
 * sections minus the loft of each one inset by the wall.
 *
 * The kernel offsets the *curve* — an arc stays an arc, a fitted curve stays
 * one curve — rather than a script offsetting points, which folds in every
 * valley narrower than the wall and hands the kernel an outline that crosses
 * itself. Where the outline's own turns are tighter than `by`, the loops the
 * offset would make are removed and the inset has a corner there; that is the
 * wall thickening into a valley, as a real one does. The result is measured
 * before it is used: an inset that does not lie exactly `by` inside the
 * outline, or that splits or vanishes, is refused naming the distance that
 * fits. An inset of an inset is refused; add the distances.
 *
 * `outline` is a section (`SectionEntry[]`): corners, arcs, curves, or one
 * closed `{ spline }` or `{ fit }`.
 */
export function inset(outline: SectionEntry[], by: number): SectionEntry[] {
  if (!(typeof by === "number" && Number.isFinite(by) && by > 0)) {
    throw new Error(`inset steps an outline inward by a distance in mm, more than 0; got ${JSON.stringify(by)}`);
  }
  checkSection(outline, "an inset outline", "inset([[-10, -10], [10, -10], [10, 10], [-10, 10]], 1.6)");
  return [{ inset: outline, by }];
}

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
 * A closed outline in XY, given a thickness along Z.
 *
 * The counterpart of {@link revolve} for a part that is drawn rather than
 * turned: a plate outline, a cam blank, a hexagon, an L-bracket, a slot. Like
 * every other primitive it is centred on the origin in Z, so the solid runs
 * from `-height / 2` to `+height / 2`; the outline carries its own placement
 * in X and Y.
 *
 * The outline is a list of `SectionEntry`: corners, rounded corners, arcs and
 * splines. A 20 × 10 slot with round ends is
 * `extrude([[-5, -5], [5, -5], { through: [10, 0] }, [5, 5], [-5, 5], { through: [-10, 0] }], 3)`,
 * and a plate with 2 mm corner radii is four `{ at: [x, y], round: 2 }`
 * corners. It may be re-entrant but must not cross itself.
 *
 * `draft` leans the walls in by that many degrees going up, and needs a convex
 * outline of straight edges: draft the polygon and `.fillet()` its vertical
 * edges for a rounded, drafted boss.
 */
export function extrude(
  profile: SectionEntry[],
  height: number,
  options: { draft?: number } = {},
): Shape {
  checkSection(profile, "an extrude outline", "extrude([[-5, -5], [5, -5], [5, 5], [-5, 5]], 2)");
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

/**
 * A helical path for `pipe` and `sweep`, in place of a list of points: a
 * spring, a coil, a thread's path, a spiral horn.
 *
 * The axis is +Z and the helix is centred on the origin like every primitive:
 * it starts at `[radius, 0, -height / 2]` and rises to `height / 2`. Give
 * `turns` or `height` (= `pitch * turns`). `endRadius` changes the radius
 * linearly with the turn angle — a conical helix, which a horn is — and
 * `hand: "left"` winds it the other way (right-handed, the default, turns
 * anticlockwise seen from above as it rises, like a standard thread).
 *
 * The kernel sweeps a curve fitted to the exact helix and measures how far
 * the two differ, refusing past 0.0001 mm. It refuses a pitch so tight the
 * turns would sweep through each other, and a radius so small the section
 * would cross the axis, naming the limit either way. Place and turn the
 * result with `.at()` and `.rotate()`. Build time grows with turns — about
 * 0.2 s a turn for a round wire. A screw thread is not a sweep: use
 * `threadedRod` or `threadedHole`, which build the ISO profile and measure it.
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
 * A smooth path for `pipe` and `sweep`, in place of a list of points: a hose,
 * a cable, a handle, a vase's rim — anything that curves without corners.
 *
 * `{ spline: [[x, y, z], ...] }` passes through every point, in order, as one
 * cubic parameterised by chord length with no curvature at its two ends (at
 * least three points; the same rule as `{ spline }` in a section). It is an
 * exact B-spline, not a chain of arcs. The section is drawn perpendicular to
 * the path at its first point, and the sweep is refused where the path bends
 * tighter than the section reaches, naming the radius and where — spread the
 * points further apart there.
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
   * The section's size at the end of the path relative to its start: `0.2`
   * ends at a fifth of the size, `2` at double, scaled about the path itself.
   * Linear in length along a path of points; on a helix, linear in turn
   * angle, which is the same thing unless `endRadius` narrows it. Must be
   * more than 0; end on a small scale such as `0.05` for a point. A tapered
   * part is B-rep only.
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
 * A round tube of `diameter` following a path: hydraulic line, hose, wire.
 *
 * This is the honest half of what Fusion calls Sweep, and it is a bigger half
 * than it first looks: the two path elements a routed tube is actually made
 * of are a straight run, which is a cylinder, and a bend, which is a partial
 * torus. Both are exact, so a routed tube is exact.
 *
 * `bend` is the centreline bend radius, which is how tube is specified and how
 * a bender is set. Without it the corners are square and filled with a ball of
 * the tube diameter — inside the swept envelope, fine for clearance work, and
 * not a shape anybody can make. With it, the runs are trimmed back to their
 * tangent points and an arc joins them, which is the real part.
 *
 * Three options take the tube off those two surfaces: `taper`, which shrinks
 * or grows the tube along its length — a strand of hair, a tail, a horn — a
 * `{ helix }` path in place of the points — a spring or a coil — and a
 * `{ spline: [[x, y, z], ...] }` path, a smooth curve through the points — a
 * hose or a cable. A tapered pipe along a path of points needs a `bend` at
 * every corner, because the ball that fills a square corner cannot taper.
 *
 * A profile that is not a circle is `sweep`.
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
 * Skin a solid through two or more outlines stacked along +Z.
 *
 * By default the walls are ruled: straight lines between consecutive
 * sections, so the surface is exactly the skin of its sections and a
 * two-section loft of an outline and its inset is the same solid a drafted
 * extrude builds. `smooth: true` fits one continuous surface through all the
 * sections instead — Fusion's default look — and the backend then measures
 * that the fit stayed inside the sections' own bounding box, refusing one
 * that bulged past it.
 *
 * The wall pairs section *edges* by index, taken literally, which makes the
 * pairing part of the intent. Every outline must resolve to the same number
 * of edges — a straight edge, an arc or a curve each count one, and a rounded
 * corner adds an arc — so a circle lofted to a square is the circle drawn as
 * four arcs between four corners, one arc to each side. Listing a section's
 * outline rotated pairs each edge with a different one above — a *twisted*
 * wall, authored on purpose. A square lofted to the same square a quarter
 * turn on is a bar twisting 90° over its length (see
 * examples/fusion360/untriangle-v3.js); the kernel is never allowed to
 * re-origin the sections to untwist what the outlines spell out.
 *
 * Outlines may be re-entrant, but must not cross themselves.
 */
export function loft(
  sections: LoftSection[],
  options: { smooth?: boolean } = {},
): Shape {
  if (!Array.isArray(sections) || sections.length < 2) {
    throw new Error("a loft needs at least 2 sections, each { z, outline }");
  }
  const allCorners = sections.every(
    (s) => Array.isArray(s?.outline) && s.outline.every((entry) => Array.isArray(entry)),
  );
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
      checkSection(section.outline as SectionEntry[], `loft section ${i}'s outline`, "[[-5, -5], [5, -5], [5, 5], [-5, 5]]");
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
  return new Shape(() => ({
    op: "loft",
    sections: sections.map(({ outline, z, point }) => (point !== undefined ? { z, point } : { outline, z })),
    ...(smooth ? { smooth } : {}),
  }), []);
}

/**
 * Sweep an outline along a path — `pipe()` with an authored section in place
 * of the circle.
 *
 * The profile is a list of `SectionEntry`: corners, arcs and curves, so a
 * stadium, a rounded rectangle or a D-shape sweeps as exactly as a square.
 *
 * A path of points is the one a bender or a router can follow: runs, and
 * tangent arcs of radius `bend` at every corner. `bend` is required as soon
 * as the path turns (an authored section has no ball to fill a square corner
 * with), must fit the legs either side, and must clear the profile's own
 * extent so the inner side of the bend does not sweep through itself. The
 * profile is drawn perpendicular to the first run, its +Y kept as close to
 * global +Z as that run allows.
 *
 * The path may instead be `{ helix: { radius, pitch, turns } }` (see
 * `HelixPath`): the profile is then drawn perpendicular to the helix at its
 * start, +X pointing away from the axis and +Y as near +Z as the helix's
 * slope allows, and it keeps that attitude to the axis all the way up — a
 * square wire wound into a coil, a thread-like ridge. Or it may be
 * `{ spline: [[x, y, z], ...] }` (see `SplinePath`), a smooth curve through
 * the points, the profile drawn perpendicular to it at the first point the
 * same way as for a run. `taper` scales the profile about the path from 1 at
 * the start to `taper` at the end.
 *
 * A *round* section should stay a `pipe()`.
 */
export function sweep(
  profile: SectionEntry[],
  path: PathPoint[] | HelixPath | SplinePath,
  options: SweepOptions = {},
): Shape {
  checkSection(profile, "a sweep profile", "[[-2, -1], [2, -1], [2, 1], [-2, 1]]");
  const bend = options.bend ?? 0;
  if (bend < 0) throw new Error("sweep bend radius must be positive");
  const taper = checkTaper(options.taper ?? 1, "sweep");
  const tapered = taper !== 1 ? { taper } : {};
  if (!Array.isArray(path)) {
    if ("spline" in path) {
      if (bend > 0) throw new Error("a spline sweep has no corners to bend; drop the bend option");
      const spline = splineSpine(path, "sweep");
      return new Shape(() => ({ op: "sweep", profile, spline, ...tapered }), []);
    }
    if (bend > 0) throw new Error("a helical sweep has no corners to bend; drop the bend option");
    const helix = helixSpine(path, "sweep");
    return new Shape(() => ({ op: "sweep", profile, helix, ...tapered }), []);
  }
  if (path.length < 2) {
    throw new Error("a sweep path needs at least 2 points, or { helix: { radius, pitch, turns } }, or { spline: [[x, y, z], ...] }");
  }
  return new Shape(() => ({
    op: "sweep",
    profile,
    path: path.map(([x, y, z]) => ({ x, y, z })),
    ...(bend > 0 ? { bend } : {}),
    ...tapered,
  }), []);
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
 * A centred `cols` x `rows` grid of points, `dx` and `dy` apart.
 *
 * `dx` is the centre-to-centre **pitch**, not the overall span: the pattern
 * runs `(cols - 1) * dx` wide, so `grid(3, 1, 20, 0)` puts points at -20, 0 and
 * +20. Every pattern in `examples/` is 2 x 2, where pitch and span happen to be
 * the same number — which is exactly why reading it as span builds a part that
 * is watertight, passes every count, and is the wrong size.
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
 * Let this script do `multiple` times the default work: put it on the first
 * line of a part that runs a simulation, a growth or a search to produce its
 * sections. Work is counted in interpreter steps (function calls plus loop
 * iterations), never timed, so a part that builds once builds on every machine
 * however busy; the refusal says how much the script was allowed. The default
 * is 600 million steps, several seconds of plain arithmetic, far more than
 * any hand-drawn part uses. A whole number from 1 to 10; calling it again only
 * ever raises the budget. Where no budget applies (the editor's own preview)
 * it does nothing.
 *
 *     scriptBudget(8);
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
 * Run a reaction-diffusion field to the pattern it settles into, and return
 * both fields — the Turing spots, stripes and lobes a generative part grows
 * its outline from. Explicit Euler steps on a ring of cells (`size: n`) or a
 * wrapping grid (`size: [w, h]`). In the sandbox it runs natively and costs
 * one step of the script's budget per cell per step, several times less than
 * the same loop written in the script; the answer is the same to the bit.
 * Refused when `dt` is too long for the diffusion to be stable, naming the
 * longest that is, and when the run diverges.
 *
 *     const { a } = simulateReactionDiffusion({
 *       model: "gierer-meinhardt", size: 100, a: noise, b: ones,
 *       diffusion: [0.3, 60], dt: 0.2 / 60, steps: 12000,
 *       kappa: 0.05, decay: [1, 1.2], source: [0.01, 0],
 *     });
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
 * Where a closed outline crosses or touches itself: every pair `[i, j]`,
 * `i < j`, of segments that meet without being neighbours, sorted; segment
 * `i` runs from point `i` to point `i + 1`, and the last back to the first.
 * Empty for an outline that is a clean loop. The check a growth or offset
 * loop makes after every move — in the sandbox it runs natively over a
 * spatial grid, so a script needs no grid of its own.
 *
 *     const bad = new Set(outlineCrossings(ring).flat());
 *     // segment i ends at point i + 1: roll back the points of the bad segments
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
 * For each point of a closed outline, the width of the gap it faces: the
 * distance to the nearest part of the outline that is at least `ignoreWithin`
 * mm away from the point *along* the outline, so the point's own stretch of
 * curve does not count. A lobe growing toward its neighbour, or a slot a wall
 * must fit into, is a gap narrower than it should be. Gaps wider than `upTo`
 * (default: no limit) come back as `Infinity`, which is also the answer for a
 * point with nothing far enough along to measure. In the sandbox it runs
 * natively over a spatial grid, so a script needs no grid of its own.
 *
 *     const gaps = outlineGaps(ring, { ignoreWithin: 6, upTo: 6 });
 *     const tooNarrow = gaps.map((g) => g < 6);
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

export interface Doc {
  units: "mm";
  root: number;
  nodes: Record<string, unknown>[];
  /** Features the graph uses that an older host cannot read; see {@link GRAPH_FEATURES}. */
  requires?: Requirement[];
}

/** A feature a graph needs, in words a host that has never heard of it can print. */
export interface Requirement {
  feature: string;
  /** The last release that cannot read it. */
  after: string;
  what: string;
}

type GraphNode = Record<string, unknown>;

function sectionEntriesOf(node: GraphNode): unknown[] {
  const lists: unknown[] = [node.profile];
  if (Array.isArray(node.sections)) {
    for (const section of node.sections) lists.push((section as GraphNode)?.outline);
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
const GRAPH_FEATURES: (Requirement & { uses: (node: GraphNode) => boolean })[] = [
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
];

function stamped(doc: Doc): Doc {
  const requires = GRAPH_FEATURES.filter((f) => doc.nodes.some(f.uses)).map(({ feature, after, what }) => ({
    feature,
    after,
    what,
  }));
  return requires.length ? { ...doc, requires } : doc;
}

/**
 * What a script may return: one shape, or an object naming each body of a
 * part that stays in several — `return { base, lid }`.
 */
export type Part = Shape | Record<string, Shape>;

// The runners (engine.ts, tools/run.ts, script.rs) say the same thing when a
// script returns neither; not exported, because an export is a reserved word.
const RETURN_HINT =
  "the script must return a shape, or an object of named shapes for a part in several bodies.\n" +
  "End it with something like:  return body.cut(hole)   or   return { base, lid }";

/**
 * Flatten a shape into the JSON graph.
 *
 * Nodes are memoised by identity, so a shape used in several places becomes one
 * node with several parents — the graph stays a DAG and the core evaluates the
 * shared work once. An object of shapes becomes one `bodies` root over each
 * body's own subgraph; a shape shared between two bodies is still one node.
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
  const entries = Object.entries(root);
  if (entries.length === 0) {
    throw new Error(
      "the script returned an empty object; return one shape, or name each body: return { base, lid }",
    );
  }
  const bodies = entries.map(([name, shape]) => {
    if (!(shape instanceof Shape)) {
      throw new Error(
        `body "${name}" is not a shape (it is ${describe(shape)}); every value in the returned object must be one`,
      );
    }
    if (!name.trim()) throw new Error("a body has an empty name; name each body: return { base, lid }");
    return { name, child: visit(shape) };
  });
  const rootId = nodes.length;
  nodes.push({ op: "bodies", bodies });
  return stamped({ units: "mm", root: rootId, nodes });
}

function describe(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return "an array";
  return typeof value === "object" ? "a plain object" : `a ${typeof value}`;
}

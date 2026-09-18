import { describe, expect, test } from "bun:test";
import {
  around,
  box,
  build,
  cone,
  countersink,
  clearance,
  counterbore,
  cylinder,
  device,
  DEVICES,
  extrude,
  grid,
  holeFor,
  loft,
  hull,
  line2d,
  ngon,
  polar,
  pipe,
  sweep,
  spurGearOutline,
  spurGearPair,
  repeat,
  revolve,
  Shape,
  tapDrill,
  threadedHole,
  threadedRod,
  torus,
  union,
  vesaPattern,
  __parcadShadowedBuiltins,
  __parcadShadowedBuiltinMessage,
} from "./dsl";
import * as everything from "./dsl";

/**
 * The pattern helpers are pure arithmetic, which is exactly why they are worth
 * testing here rather than only through `eval/cases/`: a wrong angle produces a
 * part that still builds, still measures plausibly, and is wrong on a drawing.
 */

/** Round-trip a point through the same formatting the graph would see. */
const near = (a: number, b: number) => Math.abs(a - b) < 1e-9;

describe("polar", () => {
  test("puts the first point on +X and walks anticlockwise", () => {
    const points = polar(4, 10);
    expect(points).toHaveLength(4);
    expect(near(points[0][0], 10)).toBe(true);
    expect(near(points[0][1], 0)).toBe(true);
    expect(near(points[1][0], 0)).toBe(true);
    expect(near(points[1][1], 10)).toBe(true);
  });

  test("straddle offsets by half a step, so nothing lands on a centreline", () => {
    // The ASME bolt-circle convention: four holes at 45°, not at 0°.
    for (const [x, y] of polar(4, 10, { straddle: true })) {
      expect(near(Math.abs(x), Math.abs(y))).toBe(true);
      expect(near(Math.abs(x), 10 * Math.SQRT1_2)).toBe(true);
    }
  });

  test("straddle is exactly a half-step rotation of the plain circle", () => {
    const plain = polar(6, 25, { start: 30 });
    const straddled = polar(6, 25, { straddle: true });
    // 360/6 = 60, so half a step is 30 — the two must coincide.
    for (let i = 0; i < plain.length; i++) {
      expect(near(plain[i][0], straddled[i][0])).toBe(true);
      expect(near(plain[i][1], straddled[i][1])).toBe(true);
    }
  });

  test("every point is on the circle", () => {
    for (const [x, y] of polar(7, 13.7, { start: 11 })) {
      expect(near(Math.hypot(x, y), 13.7)).toBe(true);
    }
  });

  test("refuses a count that is not a positive integer", () => {
    expect(() => polar(0, 10)).toThrow("positive integer");
    expect(() => polar(2.5, 10)).toThrow("positive integer");
  });

  test("feeds repeat() the same way grid() does", () => {
    const doc = build(repeat(cylinder(1, 5), polar(3, 20)));
    // One cylinder, three translations, one union: the shape is shared.
    const ops = doc.nodes.map((n) => n.op);
    expect(ops.filter((op) => op === "cylinder")).toHaveLength(1);
    expect(ops.filter((op) => op === "translate")).toHaveLength(3);
    expect(grid(2, 2, 4, 4)).toHaveLength(4);
  });
});

describe("around", () => {
  test("spins one shape into count copies sharing a single node", () => {
    const doc = build(around(box(2, 2, 2).at(10, 0), 4));
    const ops = doc.nodes.map((n) => n.op);
    expect(ops.filter((op) => op === "cuboid")).toHaveLength(1);
    // Three rotations, not four: the first copy is the shape itself.
    expect(ops.filter((op) => op === "rotate")).toHaveLength(3);
    expect(ops.filter((op) => op === "union")).toHaveLength(1);
  });

  test("steps by 360/count, with no identity rotation for the first copy", () => {
    const doc = build(around(box(1, 1, 1), 4));
    const angles = doc.nodes
      .filter((n) => n.op === "rotate")
      .map((n) => n.degrees as number);
    expect(angles).toEqual([90, 180, 270]);
  });

  test("takes an axis, because not every pattern is about Z", () => {
    const doc = build(around(cylinder(1, 4), 3, "x"));
    const axes = doc.nodes
      .filter((n) => n.op === "rotate")
      .map((n) => n.axis as { x: number; y: number; z: number });
    expect(axes.every((a) => a.x === 1 && a.y === 0 && a.z === 0)).toBe(true);
  });

  test("is equivalent to writing the rotations out by hand", () => {
    const slot = box(3, 3, 20).at(0, 8);
    const byHand = build(
      union(slot, slot.rotate("z", 120), slot.rotate("z", 240)),
    );
    const byHelper = build(around(slot, 3));
    expect(byHelper).toEqual(byHand);
  });

  test("refuses a count that is not a positive integer", () => {
    expect(() => around(box(1, 1, 1), -1)).toThrow("positive integer");
  });
});

describe("revolve", () => {
  test("a cone is a revolved triangle, centred like every other primitive", () => {
    const doc = build(cone(10, 4, 20));
    expect(doc.nodes).toHaveLength(1);
    expect(doc.nodes[0].op).toBe("revolve");
    expect(doc.nodes[0].profile).toEqual([
      [0, -10],
      [10, -10],
      [4, 10],
      [0, 10],
    ]);
  });

  test("a point-ended cone drops the degenerate zero-radius corner", () => {
    const profile = build(cone(6, 0, 12)).nodes[0].profile as [number, number][];
    expect(profile).toHaveLength(3);
    expect(profile.filter(([r]) => r === 0)).toHaveLength(2);
  });

  test("a countersink crosses the face it cuts, at the called-out diameter", () => {
    // The tool has to overshoot: a cutter ending exactly on the face merges
    // into it and the rim stops being an edge the cut generated.
    const doc = build(countersink(10.4, 90));
    const revolveNode = doc.nodes.find((n) => n.op === "revolve")!;
    const translate = doc.nodes.find((n) => n.op === "translate")!;
    const profile = revolveNode.profile as [number, number][];
    const dz = (translate.by as { z: number }).z;

    const top = profile[profile.length - 2];
    expect(top[1] + dz).toBeGreaterThan(0); // above the face
    // 90° included: the radius closes at 1 mm per mm, so the section at the
    // face is exactly the head radius.
    const radiusAtFace = top[0] - (top[1] + dz);
    expect(Math.abs(radiusAtFace - 5.2)).toBeLessThan(1e-9);
  });

  test("refuses a section that crosses the axis or cannot close", () => {
    expect(() => revolve([[1, 0], [2, 0]])).toThrow("at least 3");
    expect(() => revolve([[-1, 0], [2, 0], [0, 3]])).toThrow(">= 0");
    expect(() => cone(5, 5, 0)).toThrow("height must be positive");
    expect(() => countersink(6, 200)).toThrow("between 0 and 180");
  });
});

describe("sections with curves", () => {
  test("arc, round and curve entries reach the graph verbatim", () => {
    const profile = [
      { at: [0, 0] as [number, number], round: 1 },
      [10, 0] as [number, number],
      { through: [12, 5] as [number, number] },
      [10, 10] as [number, number],
      { spline: [[5, 12]] as [number, number][], start: [-1, 0] as [number, number] },
      [0, 10] as [number, number],
    ];
    expect(build(extrude(profile, 2)).nodes[0].profile).toEqual(profile);
  });

  test("a closed spline alone is a section, a lone arc is not", () => {
    expect(() => extrude([{ spline: [[0, 0], [10, 0], [5, 8]] }], 2)).not.toThrow();
    expect(() => extrude([{ through: [0, 1] }], 2)).toThrow("no corners");
  });

  test("a misspelt entry names the vocabulary", () => {
    expect(() => extrude([[0, 0], [5, 0], { thru: [5, 5] } as never, [0, 5]], 2)).toThrow('unknown key "thru"');
    expect(() => revolve([[0, 0], [5, 0], { radius: 0 }, [0, 5]])).toThrow("non-zero");
    expect(() => extrude([[0, 0], [5, 0], { bezier: [[2, 3]], degree: 2 } as never, [0, 5]], 2)).toThrow("bspline only");
  });

  test("a loft may end on a point, and only end on one", () => {
    const circle: [number, number][] | object[] = [[10, 0], { through: [0, 10] }, [-10, 0], { through: [0, -10] }];
    const doc = build(loft([{ z: 0, outline: circle as never }, { z: 10, point: [0, 0] }]));
    expect(doc.nodes[0].sections).toEqual([{ outline: circle, z: 0 }, { z: 10, point: [0, 0] }]);
    expect(() =>
      loft([{ z: 0, point: [0, 0] }, { z: 5, point: [0, 0] }, { z: 10, outline: circle as never }]),
    ).toThrow("only the first or last");
  });

  test("pipe and sweep take a spline path", () => {
    const spline = [[0, 0, 0], [10, 5, 0], [20, 0, 0]] as [number, number, number][];
    expect(build(pipe({ spline }, 2)).nodes[0]).toMatchObject({ op: "sweep", circle: 1, spline: [{ x: 0, y: 0, z: 0 }, { x: 10, y: 5, z: 0 }, { x: 20, y: 0, z: 0 }] });
    expect(() => sweep([[0, 0], [1, 0], [0, 1]], { spline: spline.slice(0, 2) })).toThrow("at least 3");
  });
});

describe("ngon", () => {
  test("across the flats is the inscribed size, across the corners the circumscribed one", () => {
    // 17 mm hex bar fits a 17 mm spanner and measures 19.63 corner to corner.
    const flats = build(ngon(6, 17, 5, { across: "flats" })).nodes[0]
      .profile as [number, number][];
    const corners = build(ngon(6, 17, 5)).nodes[0].profile as [number, number][];

    const radius = ([x, y]: [number, number]) => Math.hypot(x, y);
    expect(near(radius(flats[0]), 17 / 2 / Math.cos(Math.PI / 6))).toBe(true);
    expect(near(radius(corners[0]), 17 / 2)).toBe(true);
  });

  test("area follows the closed form, so the winding and the radius are both right", () => {
    // Shoelace over the emitted outline: a regular n-gon of circumradius R has
    // area n/2 * R^2 * sin(2pi/n), positive when the points run anticlockwise.
    const profile = build(ngon(5, 20, 3)).nodes[0].profile as [number, number][];
    let area = 0;
    for (let i = 0; i < profile.length; i++) {
      const [ax, ay] = profile[i];
      const [bx, by] = profile[(i + 1) % profile.length];
      area += ax * by - bx * ay;
    }
    const expected = (5 / 2) * 10 * 10 * Math.sin((2 * Math.PI) / 5);
    expect(near(area / 2, expected)).toBe(true);
  });

  test("refuses a shape that is not a polygon", () => {
    expect(() => ngon(2, 10, 5)).toThrow("at least 3");
    expect(() => ngon(6.5, 10, 5)).toThrow("whole sides");
    expect(() => extrude([[0, 0], [1, 0]], 2)).toThrow("at least 3");
    expect(() => extrude([[0, 0], [1, 0], [1, 1]], 0)).toThrow("must be positive");
  });
});

describe("mirror", () => {
  test("names the plane by its normal, and shares the shape it reflects", () => {
    const half = box(10, 4, 2).at(8, 0, 0);
    const doc = build(union(half, half.mirror("x")));
    const mirror = doc.nodes.find((n) => n.op === "mirror")!;

    expect(mirror.normal).toEqual({ x: 1, y: 0, z: 0 });
    // One box and one translation, referenced twice: reflecting does not copy.
    expect(doc.nodes.filter((n) => n.op === "cuboid")).toHaveLength(1);
    expect(doc.nodes.filter((n) => n.op === "translate")).toHaveLength(1);
  });

  test("is not a scale of -1", () => {
    // The distinction the graph makes, and the reason mirror is its own op: a
    // uniform -1 is a point inversion, and a non-uniform one is refused.
    const doc = build(box(2, 2, 2).mirror("z"));
    expect(doc.nodes.some((n) => n.op === "scale")).toBe(false);
  });
});

describe("fasteners", () => {
  test("knows the standard diameters, and they are not interchangeable", () => {
    // The three numbers a hole can have for one screw, all different, all
    // correct for a different job: tap it, clear it, or clear it loosely.
    expect(tapDrill("M6")).toBe(5.0);
    expect(clearance("M6", "close")).toBe(6.4);
    expect(clearance("M6")).toBe(6.6);
    expect(clearance("M6", "free")).toBe(7.0);
    expect(counterbore("M6")).toEqual({ diameter: 11, depth: 6 });
    // M2.5 is the one designation with a decimal point in it.
    expect(tapDrill("M2.5")).toBe(2.05);
  });

  test("an unknown size lists the ones it has", () => {
    expect(() => tapDrill("M7")).toThrow("Known sizes");
    expect(() => tapDrill("M7")).toThrow("M6");
  });

  test("a hole cutter crosses the face it enters, and through goes out the far side", () => {
    const blind = build(holeFor("M6", 10));
    const through = build(holeFor("M6", 10, { through: true }));
    const heightOf = (doc: ReturnType<typeof build>) =>
      doc.nodes.find((n) => n.op === "cylinder")!.h as number;

    // 0.5 of overshoot at the entry either way, and 0.5 more at the exit.
    expect(heightOf(blind)).toBe(10.5);
    expect(heightOf(through)).toBe(11);
    // The entry face stays at z = 0: the cutter hangs below it.
    const dz = (build(holeFor("M6", 10)).nodes.find((n) => n.op === "translate")!.by as { z: number }).z;
    expect(near(dz, 0.5 - 10.5 / 2)).toBe(true);
  });

  test("tapped drills the tap size, not the clearance", () => {
    const tapped = build(holeFor("M8", 12, { tapped: true }));
    expect((tapped.nodes.find((n) => n.op === "cylinder")!.r as number) * 2).toBe(6.8);
  });

  test("a threaded rod is centred and shrinks by its clearance; a threaded hole hangs below its face and grows", () => {
    const rod = build(threadedRod("M8", 20, { clearance: 0.2 })).nodes[0];
    expect(rod).toEqual({ op: "thread", diameter: 8, pitch: 1.25, from: -10, to: 10, shift: -0.2 });
    const hole = build(threadedHole("M2.5", 6, { through: true, hand: "left", clearance: 0.1 })).nodes[0];
    expect(hole).toEqual({ op: "thread", diameter: 2.5, pitch: 0.45, from: -6.5, to: 0.5, hand: "left", shift: 0.1 });
    // The tooth's phase is anchored at z = 0 of the node, so nothing is translated.
    expect(build(threadedHole("M6", 8)).nodes).toHaveLength(1);
    const tripod = build(threadedRod({ diameter: 6.35, pitch: 25.4 / 20 }, 9)).nodes[0];
    expect(tripod.pitch).toBe(1.27);
    expect(build(threadedRod("M8", 10, { pitch: 1 })).nodes[0].pitch).toBe(1);
  });

  test("a thread refuses a size it cannot read", () => {
    expect(() => threadedRod("M7", 10)).toThrow("Known sizes");
    expect(() => threadedHole("M6", 8, { clearance: -0.1 })).toThrow("radial allowance");
  });
});

describe("draft", () => {
  test("rides on the extrusion rather than being a separate operation", () => {
    const doc = build(extrude([[0, 0], [10, 0], [10, 10]], 5, { draft: 3 }));
    expect(doc.nodes).toHaveLength(1);
    expect(doc.nodes[0].draft).toBe(3);
  });

  test("ngon passes it through, and a wall angle has to be one", () => {
    expect(build(ngon(6, 20, 10, { across: "flats", draft: 2 })).nodes[0].draft).toBe(2);
    expect(() => extrude([[0, 0], [10, 0], [10, 10]], 5, { draft: 90 })).toThrow(
      "between -90 and 90",
    );
  });
});

describe("torus and pipe", () => {
  test("a torus refuses to pass through its own axis", () => {
    expect(build(torus(30, 4)).nodes[0].sweep).toBe(360);
    expect(() => torus(4, 4)).toThrow("through its own axis");
    expect(() => torus(30, 4, { sweep: 0 })).toThrow("not an arc");
  });

  test("a square-cornered pipe is runs plus one ball per corner", () => {
    const doc = build(pipe([[0, 0, 0], [60, 0, 0], [60, 40, 0]], 10));
    const ops = doc.nodes.map((n) => n.op);
    expect(ops.filter((op) => op === "cylinder")).toHaveLength(2);
    expect(ops.filter((op) => op === "sphere")).toHaveLength(1);
    expect(ops.filter((op) => op === "torus")).toHaveLength(0);
  });

  test("a bend radius replaces the ball with an arc and trims the runs", () => {
    const doc = build(pipe([[0, 0, 0], [60, 0, 0], [60, 40, 0]], 10, { bend: 15 }));
    const ops = doc.nodes.map((n) => n.op);
    expect(ops.filter((op) => op === "sphere")).toHaveLength(0);

    const arc = doc.nodes.find((n) => n.op === "torus")!;
    expect(arc.major).toBe(15);
    expect(near(arc.sweep as number, 90)).toBe(true);
    // A 90 degree bend of radius 15 eats 15 mm of straight at each end, so the
    // 60 mm run is 45 and the 40 mm run is 25.
    const lengths = doc.nodes.filter((n) => n.op === "cylinder").map((n) => n.h as number);
    expect(lengths.sort((a, b) => a - b)).toEqual([25, 45]);
  });

  test("a bend that does not fit says how big one would", () => {
    // 10 mm of straight cannot carry a 15 mm radius through a right angle.
    expect(() => pipe([[0, 0, 0], [10, 0, 0], [10, 40, 0]], 10, { bend: 15 })).toThrow(
      "does not fit",
    );
    expect(() => pipe([[0, 0, 0], [10, 0, 0], [10, 40, 0]], 10, { bend: 15 })).toThrow("10.00 mm");
  });

  test("a helical or tapered pipe is one round sweep, not runs and arcs", () => {
    const spring = build(pipe({ helix: { radius: 10, pitch: 5, height: 15, hand: "left" } }, 2));
    expect(spring.nodes).toHaveLength(1);
    expect(spring.nodes[0]).toMatchObject({
      op: "sweep",
      circle: 1,
      helix: { radius: 10, pitch: 5, turns: 3, hand: "left" },
    });
    expect(spring.nodes[0].path).toBeUndefined();

    const strand = build(pipe([[0, 0, 0], [40, 0, 0], [40, 40, 0]], 6, { bend: 10, taper: 0.25 }));
    expect(strand.nodes).toHaveLength(1);
    expect(strand.nodes[0]).toMatchObject({ op: "sweep", circle: 3, bend: 10, taper: 0.25 });

    // Untapered stays the exact composition.
    expect(build(pipe([[0, 0, 0], [0, 0, 40]], 6)).nodes[0].op).toBe("cylinder");
  });

  test("a helix or taper that is not one is refused before the graph", () => {
    expect(() => pipe({ helix: { radius: 10, pitch: 5 } }, 2)).toThrow("turns or height");
    expect(() => pipe({ helix: { radius: 10, pitch: 5, turns: 2 } }, 2, { bend: 3 })).toThrow("no corners");
    expect(() => sweep([[0, 0], [1, 0], [0, 1]], [[0, 0, 0], [0, 0, 9]], { taper: 0 })).toThrow("more than 0");
  });
});

describe("device", () => {
  test("grows every dimension and both radii by the clearance", () => {
    const body = DEVICES["macbook-pro-16"];
    const doc = build(device("macbook-pro-16", { clearance: 1 }));
    const nodes = Object.values(doc.nodes) as Array<{ op: string; size?: { x: number; y: number; z: number }; radius?: number }>;
    const cuboid = nodes.find((n) => n.op === "cuboid");
    expect(cuboid?.size).toEqual({ x: body.length + 2, y: body.width + 2, z: body.thickness + 2 });
    const radii = nodes.filter((n) => n.op === "fillet").map((n) => n.radius).sort();
    expect(radii).toEqual([body.edgeRadius + 1, body.edgeRadius + 1, body.cornerRadius + 1].sort());
  });

  test("names the table when the device is not in it", () => {
    expect(() => device("thinkpad-x1")).toThrow(/DEVICES has macbook-air-13/);
  });

  test("every entry in the table is a body its own radii fit", () => {
    // Clearance grows thickness and edge radius alike, so only the table can
    // break this; each entry is checked once, here, rather than one refusal
    // at a time.
    for (const [name, body] of Object.entries(DEVICES)) {
      expect(2 * body.edgeRadius, name).toBeLessThan(body.thickness);
      expect(2 * body.cornerRadius, name).toBeLessThan(Math.min(body.length, body.width));
      expect(body.source, name).toMatch(/spec/);
    }
  });
});

describe("vesaPattern", () => {
  test("is the square pattern centred on the origin", () => {
    expect(vesaPattern(100).map(([x, y]) => [Math.abs(x), Math.abs(y)])).toEqual([
      [50, 50], [50, 50], [50, 50], [50, 50],
    ]);
    expect(vesaPattern(75).length).toBe(4);
  });

  test("refuses a size that is not a VESA pattern", () => {
    expect(() => vesaPattern(90 as 75)).toThrow(/75, 100 or 200/);
  });
});

describe("line2d", () => {
  test("offsets to the left of travel and meets another line where geometry says", () => {
    const base = line2d([0, 0], [10, 0]);
    expect(base.offset(2).from).toEqual([0, 2]);
    expect(base.offset(-2).to).toEqual([10, -2]);
    const up = line2d([4, -5], 90);
    expect(up.meet(base).map((v) => +v.toFixed(9))).toEqual([4, 0]);
    expect(base.pointAt(3)).toEqual([3, 0]);
    expect(+line2d([0, 0], [3, 4]).length().toFixed(9)).toBe(5);
  });

  test("refuses parallel lines and degenerate ones", () => {
    expect(() => line2d([0, 0], [1, 0]).meet(line2d([0, 1], [1, 1]))).toThrow(/parallel/);
    expect(() => line2d([1, 1], [1, 1])).toThrow(/distinct/);
  });

  test("reads y at x and x at y along a slope", () => {
    const slope = line2d([0, 0], [10, 5]);
    expect(slope.yAt(4)).toBe(2);
    expect(slope.xAt(2)).toBe(4);
  });
});

describe("hull", () => {
  test("is the anticlockwise convex outline with inside points dropped", () => {
    const outline = hull([[0, 0], [10, 0], [10, 10], [0, 10], [5, 5], [2, 3]]);
    expect(outline).toEqual([[0, 0], [10, 0], [10, 10], [0, 10]]);
    // Anticlockwise: the signed area is positive.
    let area = 0;
    for (let i = 0; i < outline.length; i++) {
      const [x1, y1] = outline[i], [x2, y2] = outline[(i + 1) % outline.length];
      area += x1 * y2 - x2 * y1;
    }
    expect(area).toBeGreaterThan(0);
  });

  test("a fan from a band's cross-section to a corner is convex by construction", () => {
    const fan = hull([[95.9, -79.1], [130.9, -43.3], [122.85, -124.05], [182.85, -129.05], [182.85, -29.05]]);
    expect(fan.length).toBe(5);
    expect(() => build(extrude(fan, 10))).not.toThrow();
  });

  test("refuses fewer than three distinct points, and collinear ones", () => {
    expect(() => hull([[0, 0], [1, 1]])).toThrow(/three distinct/);
    expect(() => hull([[0, 0], [1, 1], [2, 2]])).toThrow(/collinear/);
  });
});

describe("bodies", () => {
  test("an object of shapes becomes one bodies root over each body's subgraph", () => {
    const base = box(60, 40, 20).tag("base");
    const lid = box(60, 40, 3).at(0, 0, 21).tag("lid");
    const doc = build({ base, lid });
    const root = doc.nodes[doc.root];
    expect(root.op).toBe("bodies");
    const bodies = root.bodies as { name: string; child: number }[];
    expect(bodies.map((b) => b.name)).toEqual(["base", "lid"]);
    expect(doc.nodes[bodies[0].child].tag).toBe("base");
    expect(doc.nodes[bodies[1].child].tag).toBe("lid");
    // The root is the last node, after every body.
    expect(doc.root).toBe(doc.nodes.length - 1);
  });

  test("a shape shared between two bodies is still one node", () => {
    const bore = cylinder(3, 40);
    const doc = build({ left: box(20, 20, 20).cut(bore), right: box(20, 20, 20).at(30, 0, 0).cut(bore) });
    expect(doc.nodes.filter((n) => n.op === "cylinder")).toHaveLength(1);
  });

  test("one shape still builds the graph it always did", () => {
    const doc = build(box(1, 2, 3));
    expect(doc.nodes.some((n) => n.op === "bodies")).toBe(false);
    expect(doc.root).toBe(0);
  });

  test("refuses an array, an empty object, a non-shape body and an empty name, naming the fix", () => {
    expect(() => build([box(1, 1, 1)] as unknown as Record<string, Shape>)).toThrow(/return \{ left, right \}/);
    expect(() => build({})).toThrow(/return \{ base, lid \}/);
    expect(() => build({ a: box(1, 1, 1), b: 5 as unknown as Shape })).toThrow(/body "b" is not a shape \(it is a number\)/);
    expect(() => build({ " ": box(1, 1, 1) })).toThrow(/empty name/);
  });
});

/** A clamped B-spline evaluated by de Boor, independent of the code that drew it. */
function deBoor(poles: number[][], knots: number[], degree: number, u: number): number[] {
  const n = poles.length - 1;
  let k = degree;
  while (k < n && knots[k + 1] <= u) k++;
  const d = Array.from({ length: degree + 1 }, (_, j) => [...poles[j + k - degree]]);
  for (let r = 1; r <= degree; r++) {
    for (let j = degree; j >= r; j--) {
      const lo = knots[j + k - degree];
      const a = (u - lo) / (knots[j + 1 + k - r] - lo);
      d[j] = d[j].map((v, i) => (1 - a) * d[j - 1][i] + a * v);
    }
  }
  return d[degree];
}

type Drawn = { bspline: number[][]; knots: number[]; within: number; certified: boolean; check: number[][] };

/** The corners and drawn curves of an extruded outline's graph. */
function drawnOutline(outline: Parameters<typeof extrude>[0]) {
  const profile = build(extrude(outline, 1)).nodes[0].profile as (number[] | Drawn)[];
  const curves: { poles: number[][]; drawn: Drawn }[] = [];
  profile.forEach((entry, i) => {
    if (Array.isArray(entry) || !("within" in entry)) return;
    const before = profile[i - 1] as number[];
    const after = profile[(i + 1) % profile.length] as number[];
    curves.push({ poles: [before, ...entry.bspline, after], drawn: entry });
  });
  return { profile, curves };
}

/** The furthest a drawn curve strays from `truth`, sampled densely in its own parameter. */
function worstStray(poles: number[][], knots: number[], truth: (s: number) => number[], samples = 4000) {
  const end = knots[knots.length - 1];
  let worst = 0;
  for (let i = 0; i <= samples; i++) {
    const s = (end * i) / samples;
    const p = deBoor(poles, knots, 3, s);
    const q = truth(s);
    worst = Math.max(worst, Math.hypot(p[0] - q[0], p[1] - q[1]));
  }
  return worst;
}

describe("curve section entries", () => {
  const circle = (t: number): [number, number] => [20 * Math.cos(t), 20 * Math.sin(t)];

  test("a certified circle is within the bound it states everywhere, not only at its points", () => {
    const { profile, curves } = drawnOutline([
      { curve: circle, derivative: (t) => [-20 * Math.sin(t), 20 * Math.cos(t)], fourth: () => 20, from: 0, to: Math.PI * 2, tolerance: 0.001 },
    ]);
    // One closed curve: its start is the only corner.
    expect(profile).toHaveLength(2);
    const [{ poles, drawn }] = curves;
    expect(drawn.certified).toBe(true);
    expect(drawn.within).toBeLessThanOrEqual(0.001);
    // Sixteen pieces would be 0.00175 mm off; 32 of 2π/32 are
    // √2 · 20 · h⁴ / 384, plus the rounding allowance.
    const h = (Math.PI * 2) / 32;
    expect(drawn.within).toBeCloseTo((Math.SQRT2 * 20 * h ** 4) / 384 + 1e-9, 12);
    expect(drawn.knots).toHaveLength(2 * 32 + 6);
    const stray = worstStray(poles, drawn.knots, circle);
    expect(stray).toBeLessThanOrEqual(drawn.within);
    expect(stray).toBeGreaterThan(drawn.within / 4);
    expect(drawn.check).toHaveLength(3 * 32);
  });

  test("a bare function is drawn with an estimated bound, and a backwards range runs backwards", () => {
    const { profile, curves } = drawnOutline([[-10, 0], [10, 0], { curve: (t) => [10 * Math.cos(t), 10 * Math.sin(t)], from: 0, to: Math.PI, tolerance: 0.0001 }]);
    // [10, 0] is the curve's start and is merged with it; [-10, 0] is its end.
    expect(profile.filter(Array.isArray)).toHaveLength(2);
    const [{ poles, drawn }] = curves;
    expect(drawn.certified).toBe(false);
    expect(worstStray(poles, drawn.knots, (s) => [10 * Math.cos(s), 10 * Math.sin(s)])).toBeLessThanOrEqual(2 * drawn.within);
    const back = drawnOutline([[0, 0], { curve: circle, from: Math.PI / 2, to: 0, tolerance: 0.001 }]).curves[0];
    expect(back.poles[0][0]).toBeCloseTo(0, 12);
    expect(back.poles[back.poles.length - 1][0]).toBeCloseTo(20, 12);
  });

  test("a curve that cannot be certified as given is refused, naming which half is wrong", () => {
    const derivative = (t: number): [number, number] => [-20 * Math.sin(t), 20 * Math.cos(t)];
    const range = { from: 0, to: Math.PI, tolerance: 0.001 };
    expect(() => extrude([[0, 0], { curve: circle, fourth: () => 20, ...range }], 1)).toThrow(/beside its exact derivative/);
    expect(() => extrude([[0, 0], { curve: circle, derivative, fourth: () => 0.01, ...range }], 1)).toThrow(/fourth must be at least the length of the fourth derivative/);
    expect(() => extrude([[0, 0], { curve: circle, derivative: (t) => [-10 * Math.sin(t), 20 * Math.cos(t)], fourth: () => 20, ...range }], 1)).toThrow(/must be the exact derivative/);
    expect(() => extrude([[0, 0], { curve: (t) => [t, Math.sqrt(1 - t)], from: 0, to: 2, tolerance: 0.001 }], 1)).toThrow(/returned \[1\.0+\d*,"NaN"\]/);
    expect(() => extrude([[0, 0], { curve: circle, from: 0, to: 0, tolerance: 0.001 }], 1)).toThrow(/two different finite numbers/);
    expect(() => extrude([[0, 0], { curve: circle, from: 0, to: 1, tolerance: 0 }], 1)).toThrow(/at least 0.000001/);
  });

  test("a script cannot state a bound for a curve it did not draw from a function", () => {
    const claimed = { bspline: [[10, 5]], within: 0, certified: true, check: [[10, 5]] } as unknown as Parameters<typeof extrude>[0][number];
    expect(() => extrude([[0, 0], [10, 0], claimed, [0, 10]], 1)).toThrow(/a script cannot state it/);
  });
});

describe("spurGearOutline", () => {
  const m = 2;
  const z = 20;
  const alpha = (20 * Math.PI) / 180;
  const base = ((m * z) / 2) * Math.cos(alpha);

  test("every flank is certified and lies on the involute of the base circle", () => {
    const { curves } = drawnOutline(spurGearOutline({ module: m, teeth: z, tolerance: 1e-4 }));
    expect(curves).toHaveLength(2 * z);
    const inv = (a: number) => Math.tan(a) - a;
    const half = Math.PI / (2 * z) + inv(alpha);
    const tip = Math.sqrt((22 / base) ** 2 - 1);
    for (const [i, { poles, drawn }] of curves.entries()) {
      expect(drawn.certified).toBe(true);
      expect(drawn.within).toBeLessThanOrEqual(1e-4);
      const centre = (2 * Math.PI * Math.floor(i / 2)) / z;
      // Rising flanks start on the base circle; falling ones start at the tip.
      const rising = i % 2 === 0;
      const truth = (s: number) => {
        const t = rising ? s : tip - s;
        const a = rising ? centre - half + t : centre + half - t;
        const sign = rising ? 1 : -1;
        return [base * (Math.cos(a) + sign * t * Math.sin(a)), base * (Math.sin(a) - sign * t * Math.cos(a))];
      };
      expect(worstStray(poles, drawn.knots, truth, 800)).toBeLessThanOrEqual(drawn.within);
      const radii = [poles[0], poles[poles.length - 1]].map((p) => Math.hypot(p[0], p[1]));
      expect(radii[rising ? 0 : 1]).toBeCloseTo(base, 9);
      expect(radii[rising ? 1 : 0]).toBeCloseTo(22, 9);
    }
  });

  test("refuses a gear a hob would undercut, naming the least shift, and teeth that come to a point", () => {
    expect(() => spurGearOutline({ module: 1, teeth: 17 })).toThrow(/1 − \(17 \/ 2\) · sin²\(20°\) = 0\.0057, and profileShift is 0.*does not draw the trochoid.*profileShift: 0\.006 or more, 18 or more teeth/);
    expect(() => spurGearOutline({ module: 1, teeth: 17, profileShift: 0.006 })).not.toThrow();
    expect(() => spurGearOutline({ module: 1, teeth: 12, profileShift: 0.29 })).toThrow(/= 0\.2981, and profileShift is 0\.29.*profileShift: 0\.299 or more, 13 or more teeth/);
    expect(() => spurGearOutline({ module: 1, teeth: 12, pressureAngle: 25 })).not.toThrow();
    expect(() => spurGearOutline({ module: 1, teeth: 11, pressureAngle: 25 })).toThrow(/a hob undercuts 11 teeth at 25°/);
    // A deeper root is cut by a deeper hob, which undercuts sooner.
    expect(() => spurGearOutline({ module: 1, teeth: 18, dedendum: 1.4 })).toThrow(/1\.15 \(dedendum \/ module − 0\.25\)/);
    expect(() => spurGearOutline({ module: 1, teeth: 20, addendum: 1.8 })).toThrow(/come to a point at radius 11\.5\d+ mm, inside the 11\.800 mm tip circle \(1\.5\d+ mm outside the reference circle\)/);
    expect(() => spurGearOutline({ module: 1, teeth: 8, profileShift: 1.2 })).toThrow(/come to a point/);
    expect(() => spurGearOutline({ module: 0, teeth: 20 })).toThrow(/module is the reference diameter/);
    expect(() => spurGearOutline({ module: 1, teeth: 20, profileShift: Number.NaN })).toThrow(/profileShift is the shift coefficient x/);
  });

  test("a profile shift moves tip and root out and thickens the tooth by 2 x m tan α at the reference circle", () => {
    const shift = 0.3;
    const teeth = 12;
    const pitch = (m * teeth) / 2;
    const rb = pitch * Math.cos(alpha);
    const { profile, curves } = drawnOutline(spurGearOutline({ module: m, teeth, profileShift: shift }));
    const radii = profile.filter((e): e is number[] => Array.isArray(e)).map((p) => Math.hypot(p[0], p[1]));
    expect(Math.min(...radii)).toBeCloseTo(pitch - m * (1.25 - shift), 9);
    expect(Math.max(...radii)).toBeCloseTo(pitch + m * (1 + shift), 9);
    // Tooth 0's rising flank crosses the reference circle at half the tooth's thickness below +X.
    const thickness = m * (Math.PI / 2 + 2 * shift * Math.tan(alpha));
    const inv = (a: number) => Math.tan(a) - a;
    const half = thickness / (2 * pitch) + inv(alpha);
    const { poles, drawn } = curves[0];
    expect(drawn.certified).toBe(true);
    const truth = (s: number) => [rb * (Math.cos(s - half) + s * Math.sin(s - half)), rb * (Math.sin(s - half) - s * Math.cos(s - half))];
    expect(worstStray(poles, drawn.knots, truth, 800)).toBeLessThanOrEqual(drawn.within);
    const atReference = truth(Math.tan(alpha));
    expect(Math.hypot(atReference[0], atReference[1])).toBeCloseTo(pitch, 12);
    expect(Math.atan2(atReference[1], atReference[0])).toBeCloseTo(-thickness / (2 * pitch), 12);
  });
});

describe("spurGearPair", () => {
  const inv = (a: number) => Math.tan(a) - a;
  const alpha = (20 * Math.PI) / 180;

  test("an unshifted pair sits at m (z1 + z2) / 2, and the second gear turns half a tooth only for an even count", () => {
    const even = spurGearPair({ module: 2, teeth: [20, 30] });
    expect(even.centres).toBeCloseTo(50, 12);
    expect(even.pressureAngle).toBeCloseTo(20, 10);
    expect(even.turn).toBeCloseTo(6, 12);
    expect(even.outlines).toHaveLength(2);
    expect(spurGearPair({ module: 2, teeth: [20, 31] }).turn).toBe(0);
    // Equal and opposite shifts keep the standard centre distance.
    expect(spurGearPair({ module: 2, teeth: [20, 30], profileShift: [0.4, -0.4] }).centres).toBeCloseTo(50, 10);
  });

  test("a shifted pair moves apart by the working pressure angle and keeps 0.25 m of tip clearance", () => {
    const pair = spurGearPair({ module: 2, teeth: [12, 30], profileShift: [0.3, 0] });
    const working = (pair.pressureAngle * Math.PI) / 180;
    expect(inv(working)).toBeCloseTo(inv(alpha) + (2 * Math.tan(alpha) * 0.3) / 42, 14);
    expect(pair.centres).toBeCloseTo((42 * Math.cos(alpha)) / Math.cos(working), 12);
    expect(pair.centres).toBeCloseTo(42.5719, 4);
    const radii = pair.outlines.map((outline) =>
      outline.filter((e): e is [number, number] => Array.isArray(e)).map((p) => Math.hypot(p[0], p[1])),
    );
    const tips = pair.outlines.map((outline) =>
      Math.max(...outline.map((e) => ("through" in (e as object) ? Math.hypot(...(e as { through: [number, number] }).through) : 0))),
    );
    const roots = radii.map((r) => Math.min(...r));
    expect(pair.centres - tips[0] - roots[1]).toBeCloseTo(0.5, 12);
    expect(pair.centres - tips[1] - roots[0]).toBeCloseTo(0.5, 12);
  });

  test("refuses a pair that would lose contact or jam, and passes an undercut refusal through", () => {
    expect(() => spurGearPair({ module: 2, teeth: [10, 10], profileShift: [1, 1] })).toThrow(/contact ratio of this pair is 0\.775, under 1/);
    expect(() => spurGearPair({ module: 2, teeth: [12, 30] })).toThrow(/spurGearPair's 12-tooth gear: a hob undercuts 12 teeth/);
    expect(() => spurGearPair({ module: 1, teeth: [8, 40], profileShift: [0.3, -1], pressureAngle: 25 })).toThrow(
      /the 40-tooth gear's tip reaches the 8-tooth gear below its base circle along the line of action.*Shift the 8-tooth gear out further/,
    );
    expect(() => spurGearPair({ module: 1, teeth: [200, 200], profileShift: [-5, -5] })).toThrow(/meet at no centre distance: x1 \+ x2 must be more than .* = -8\.1\d+/);
    expect(() => spurGearPair({ module: 2, teeth: [12, 30], profileShift: [0.3] as unknown as [number, number] })).toThrow(/profileShift is \[first, second\]/);
    expect(() => spurGearPair({ module: 2, teeth: 12 as unknown as [number, number] })).toThrow(/teeth is \[first, second\]/);
  });
});

describe("selector arguments", () => {
  const part = box(10, 10, 10);
  // Scripts are plain JavaScript: a missing or wrong argument gets past no type checker.
  const loose = part as unknown as Record<string, (...args: unknown[]) => unknown>;

  function refusal(run: () => unknown): Error {
    try {
      run();
    } catch (e) {
      expect(e).not.toBeInstanceOf(TypeError);
      return e as Error;
    }
    throw new Error("expected a refusal");
  }

  /** Each call a refusal tells the script to write, run on the part it refused. */
  function namedFixesBuild(message: string) {
    const fixes = [...message.matchAll(/\.(?:edges|vertices|fillet|chamfer)\([^()]*\)/g)].map((m) => m[0]);
    expect(fixes).toHaveLength(2);
    for (const fix of fixes) {
      const made = new Function("part", `return part${fix};`)(part) as Shape | { fillet(radius: number): Shape };
      build(made instanceof Shape ? made : made.fillet(1));
    }
  }

  test("edges() with no selector is refused, not read as every edge, and names selectors that work", () => {
    const { message } = refusal(() => loose.edges());
    expect(message).toBe(
      'edges() needs a selector; leaving it out does not select every edge. Write .edges(">Z") for the edges furthest in +Z, or .edges({ dihedral: "convex" }) for every outside edge.',
    );
    namedFixesBuild(message);
  });

  test("a treatment with no selector names the second argument, at the size the script gave", () => {
    const fillet = refusal(() => loose.fillet(2)).message;
    expect(fillet).toBe(
      'fillet(2) needs a selector; leaving it out does not select every edge. Write .fillet(2, ">Z") for the edges furthest in +Z, or .fillet(2, { dihedral: "convex" }) for every outside edge.',
    );
    namedFixesBuild(fillet);
    const chamfer = refusal(() => loose.chamfer(1)).message;
    expect(chamfer).toContain('Write .chamfer(1, ">Z")');
    namedFixesBuild(chamfer);
    expect(refusal(() => loose.fillet()).message).toContain('.fillet(radius, { dihedral: "convex" })');
  });

  test("vertices() with no selector is refused and points at the corner and edge forms", () => {
    const { message } = refusal(() => loose.vertices());
    expect(message).toBe(
      'vertices() needs a selector; leaving it out does not select every corner. Write .vertices(">X and >Y and >Z") for the corner furthest in +X, +Y and +Z, or .edges({ dihedral: "convex" }) for every outside edge.',
    );
    namedFixesBuild(message);
  });

  test("a value that is not a selector is named, not read as an empty query", () => {
    const values: [unknown, string][] = [
      [null, "null"],
      [5, "5"],
      [[">Z"], "an array"],
      [box(1, 1, 1), "a shape"],
      [() => ">Z", "a function"],
    ];
    for (const [value, named] of values) {
      expect(refusal(() => loose.edges(value)).message).toStartWith(`edges() takes a selector, not ${named}. Write .edges(">Z")`);
      expect(refusal(() => loose.vertices(value)).message).toStartWith(`vertices() takes a selector, not ${named}. Write .vertices(`);
    }
    expect(refusal(() => loose.chamfer(1, null)).message).toStartWith("chamfer(1) takes a selector, not null.");
  });

  test("a query key the language does not have is refused with where it goes, and the fix builds", () => {
    expect(refusal(() => loose.edges({ at: { z: "max" }, faceNormal: "+z" })).message).toBe(
      'an edge query has no key "faceNormal" (write adjacentTo: { faceNormal: "+z" } instead). Its keys are generatedBy, curve, role, adjacentTo, at, dihedral, parallel, longerThan, on and between.',
    );
    build(part.edges({ at: { z: "max" }, adjacentTo: { faceNormal: "+z" } }).fillet(1));

    expect(refusal(() => loose.fillet(1, { at: { z: "max" }, count: 4 })).message).toStartWith(
      'an edge query has no key "count" (write .expect({ count: 4 }) on the selection instead).',
    );
    build(part.edges({ at: { z: "max" } }).expect({ count: 4 }).fillet(1));

    expect(refusal(() => loose.vertices({ at: { z: "max" }, dihedral: "convex" })).message).toBe(
      'a vertex query has no key "dihedral". Its only key is at, e.g. { at: { z: "max" } }.',
    );
  });

  test("a key the empty-query check would have blamed is named instead", () => {
    expect(refusal(() => loose.edges({ direction: ">Z" })).message).toStartWith('an edge query has no key "direction".');
  });

  test("at, adjacentTo and curve take only the values the kernel reads", () => {
    expect(refusal(() => loose.edges({ at: { z: "top" } })).message).toBe('at.z must be "min" or "max", not "top".');
    expect(refusal(() => loose.vertices({ at: { w: "max" } })).message).toStartWith('at has no key "w".');
    expect(refusal(() => loose.edges({ adjacentTo: { faceNormal: "+Z" } })).message).toEndWith('not "+Z" (write "+z" instead).');
    expect(refusal(() => loose.edges({ curve: "arc" })).message).toBe('curve must be "line", "circle" or "spline", not "arc"');
    build(part.edges({ at: { x: undefined, z: "min" }, curve: "line" }).chamfer(1));
  });
});

/**
 * Every export is a parameter of every script, so a local called `clearance`
 * is a SyntaxError from an engine that names no identifier. The collision is
 * proved by compiling, never read off the text: docs/DSL_GAPS.md §7.
 */
describe("a script that declares one of parcad's names", () => {
  const names = Object.keys(everything);

  test("is told which name, proved by compiling without it", () => {
    expect(__parcadShadowedBuiltins("const clearance = 0.6;\nreturn box(1, 1, 1);", names)).toEqual(["clearance"]);
    expect(__parcadShadowedBuiltins("let hull = 2, box = 3;\nreturn sphere(1);", names)).toEqual(["box", "hull"]);
    expect(__parcadShadowedBuiltins("class Shape {}\nreturn sphere(1);", names)).toEqual(["Shape"]);
    // A function declaration may shadow a parameter, so it is an override, not a fault.
    expect(__parcadShadowedBuiltins("function torus() {}\nreturn sphere(1);", names)).toEqual([]);
  });

  test("a script with a fault of its own names nothing", () => {
    expect(__parcadShadowedBuiltins("return box(1, 1, 1;", names)).toEqual([]);
    expect(__parcadShadowedBuiltins("const closest = 0.6;\nreturn box(1, 1, 1);", names)).toEqual([]);
  });

  test("the refusal names the rename and the builtin", () => {
    const message = __parcadShadowedBuiltinMessage(["clearance"], names);
    expect(message).toStartWith(`\`clearance\` is one of the ${names.length} names parcad puts in every script, so a script cannot declare it again. Rename the local — \`clearanceMm\`, \`myClearance\`, or a name saying what it holds — or use parcad's own \`clearance\` instead of declaring one.`);
    expect(__parcadShadowedBuiltinMessage(["box", "hull"], names)).toStartWith("`box` and `hull` are 2 of the");
  });
});

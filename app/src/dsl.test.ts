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
  extrude,
  grid,
  holeFor,
  ngon,
  polar,
  pipe,
  repeat,
  revolve,
  tapDrill,
  torus,
  union,
} from "./dsl";

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
});

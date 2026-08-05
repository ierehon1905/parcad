import { describe, expect, test } from "bun:test";
import {
  around,
  box,
  build,
  cone,
  countersink,
  cylinder,
  extrude,
  grid,
  ngon,
  polar,
  repeat,
  revolve,
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

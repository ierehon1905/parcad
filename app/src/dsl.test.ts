import { describe, expect, test } from "bun:test";
import { around, box, build, cylinder, grid, polar, repeat, union } from "./dsl";

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

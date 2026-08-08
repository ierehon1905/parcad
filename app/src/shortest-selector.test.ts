import { describe, expect, test } from "bun:test";

import { shortestUniqueSelector } from "./shortest-selector";

/** The eight corners of a 20 mm cube, which is the smallest set with ties. */
const corners = [
  [-10, -10, -10],
  [-10, -10, 10],
  [-10, 10, -10],
  [-10, 10, 10],
  [10, -10, -10],
  [10, -10, 10],
  [10, 10, -10],
  [10, 10, 10],
].map((point) => ({ point }));

const of = (item: { point: number[] }, eps = 1e-4) =>
  shortestUniqueSelector(item, corners, (c) => c.point, eps);

describe("the shortest unique selector", () => {
  test("needs all three axes to name one corner of a cube", () => {
    // Every corner is extreme on all three, so no shorter conjunction exists.
    expect(of(corners[7])).toBe(">X and >Y and >Z");
    expect(of(corners[0])).toBe("<X and <Y and <Z");
  });

  test("stops as soon as one candidate is left", () => {
    const pair = [{ point: [0, 0, 0] }, { point: [5, 0, 0] }];
    expect(shortestUniqueSelector(pair[1], pair, (c) => c.point, 1e-4)).toBe(">X");
  });

  test("says nothing when the item is not distinguishable", () => {
    // A point in the middle is extreme on no axis, so it earns no term.
    const set = [...corners, { point: [0, 0, 0] }];
    expect(shortestUniqueSelector(set[8], set, (c) => c.point, 1e-4)).toBeUndefined();
  });

  test("says nothing for an empty set", () => {
    expect(shortestUniqueSelector({ point: [0, 0, 0] }, [], (c) => c.point, 1e-4)).toBeUndefined();
  });

  test("a looser tolerance makes near-coincident items tie", () => {
    // The whole reason epsilon is a parameter: at the vertex grid these two are
    // the same point and neither can be named, at the edge tolerance they part.
    const near = [{ point: [0, 0, 0] }, { point: [0.0005, 0, 0] }];
    expect(shortestUniqueSelector(near[1], near, (c) => c.point, 1e-3)).toBeUndefined();
    expect(shortestUniqueSelector(near[1], near, (c) => c.point, 1e-4)).toBe(">X");
  });

  test("uses a directional term when one is offered", () => {
    const edges = [
      { center: [0, 0, 0], along: 2 },
      { center: [0, 0, 0], along: 0 },
    ];
    const selector = shortestUniqueSelector(edges[0], edges, (e) => e.center, 1e-4, {
      of: (e) => `|${["X", "Y", "Z"][e.along]}`,
      matches: (candidate, axis) => candidate.along === axis,
    });
    // Same centre, so only the direction can tell them apart.
    expect(selector).toBe("|Z");
  });
});

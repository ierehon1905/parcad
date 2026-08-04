import { expect, test } from "bun:test";
import { suggestVertexSelector, verticesFromEdges } from "../src/entities.ts";

test("visible edge endpoints become stable vertex IDs while closed rims stay out", () => {
  const vertices = verticesFromEdges([
    { points: [[0, 0, 0], [10, 0, 0]] },
    { points: [[0, 0, 0], [0, 10, 0]] },
    { points: [[5, 5, 0], [5, 5, 0]] },
  ]);

  expect(vertices).toEqual([
    { id: "vertex@0", point: [0, 0, 0], degree: 2 },
    { id: "vertex@1", point: [0, 10, 0], degree: 1 },
    { id: "vertex@2", point: [10, 0, 0], degree: 1 },
  ]);
});

test("a vertex inspector suggests only extrema accepted by vertices()", () => {
  const vertices = verticesFromEdges([
    { points: [[1, 1, 1], [1, -1, 1]] },
    { points: [[1, 1, 1], [-1, 1, 1]] },
    { points: [[1, 1, 1], [1, 1, -1]] },
  ]);
  const corner = vertices.find((vertex) => vertex.point.every((value) => value === 1));

  expect(corner).toBeDefined();
  expect(suggestVertexSelector(corner, vertices)).toBe(">X and >Y and >Z");
});

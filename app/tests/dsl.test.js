import { expect, test } from "bun:test";
import { box, build } from "../src/dsl.ts";

test("a vertex treatment preserves a semantic corner selector in the graph", () => {
  const part = box(10, 10, 10)
    .vertices(">X and >Y and >Z")
    .expect({ count: 1 })
    .fillet(1);
  const graph = JSON.parse(JSON.stringify(build(part)));

  expect(graph.nodes[1]).toMatchObject({
    op: "fillet",
    child: 0,
    radius: 1,
    vertices: ">X and >Y and >Z",
    expect: { count: 1 },
  });
  expect(graph.nodes[1]).not.toHaveProperty("selector");
});

test("vertex selectors reject edge-only direction terms", () => {
  expect(() => box(10).vertices(">X and |Y")).toThrow("|X applies to edges");
});

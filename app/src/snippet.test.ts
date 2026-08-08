/**
 * The first-argument scan, measured rather than assumed.
 *
 * `extrude([[-10, -5], …], 3)` has commas *inside* its first argument, so
 * anything that split on the first comma would hand the user a broken
 * expression to type over.
 */

import { describe, expect, test } from "bun:test";

import { firstArgumentEnd } from "./snippet";

/** What pressing this snippet would leave selected. */
function firstArgument(snippet: string): string {
  const opening = snippet.indexOf("(");
  const start = opening + 1;
  return snippet.slice(start, firstArgumentEnd(snippet, start));
}

describe("the first-argument scan", () => {
  test("stops at a top-level comma", () => {
    expect(firstArgument("box(30, 20, 10)")).toBe("30");
  });

  test("stops at the closing paren of a one-argument call", () => {
    expect(firstArgument("sphere(8)")).toBe("8");
  });

  test("steps over commas nested in brackets", () => {
    expect(firstArgument("extrude([[-10, -5], [10, -5]], 3)")).toBe("[[-10, -5], [10, -5]]");
  });

  test("steps over commas and parens inside a string", () => {
    expect(firstArgument('.edges(">Z and |X")')).toBe('">Z and |X"');
    expect(firstArgument('tag("a, b)")')).toBe('"a, b)"');
  });

  test("takes the whole object when the first argument is one", () => {
    expect(firstArgument("f({ a: 1, b: 2 }, 3)")).toBe("{ a: 1, b: 2 }");
  });

  test("reports no argument when the call never closes", () => {
    // Back to `start`, not to the end of the text. An empty span is what makes
    // `insertSnippet` fall back to parking the caret after the call, rather
    // than selecting the tail of a snippet it failed to parse.
    expect(firstArgumentEnd("box(30", 4)).toBe(4);
  });
});

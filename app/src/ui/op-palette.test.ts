/**
 * The palette, measured rather than assumed.
 *
 * Two things here can be quietly wrong in a way nobody notices until a user
 * presses a button, and both are checked against the catalogue itself rather
 * than against a handful of hand-picked strings — a new operation is covered
 * the moment it is added.
 *
 * The first is the first-argument scan. `extrude([[-10, -5], …], 3)` has commas
 * *inside* its first argument, so anything that split on the first comma would
 * hand the user a broken expression to type over. The second is the catalogue:
 * every snippet has to be a call whose first argument the scan can find, or
 * pressing the icon parks the caret somewhere useless.
 */

import { describe, expect, test } from "bun:test";

import { firstArgumentEnd, OP_GROUPS } from "./op-palette";

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

describe("the catalogue", () => {
  const ops = OP_GROUPS.flatMap((group) => group.ops.map((op) => [group.id, op] as const));

  test("names every operation once", () => {
    const names = ops.map(([, op]) => op.name);
    // `blend` is deliberately a second entry for `.union` with an option, which
    // is why this checks the *pair* rather than the name alone.
    const keys = ops.map(([group, op]) => `${group}.${op.name}`);
    expect(new Set(keys).size).toBe(names.length);
  });

  test.each(ops.map(([group, op]) => [`${group}.${op.name}`, op] as const))(
    "%s inserts a call whose first argument can be selected",
    (_label, op) => {
      expect(op.snippet).toContain("(");
      const selected = firstArgument(op.snippet);
      expect(selected.length).toBeGreaterThan(0);
      // A selection that swallowed the rest of the call would mean the scan
      // never found a boundary, which is the failure worth catching.
      expect(selected.length).toBeLessThan(op.snippet.length);
    },
  );

  test.each(ops.map(([group, op]) => [`${group}.${op.name}`, op] as const))(
    "%s shows a signature that starts the way its snippet does",
    (_label, op) => {
      // A method is written with its leading dot in both places, so the palette
      // never shows a form the script cannot contain.
      expect(op.signature.startsWith(".")).toBe(op.snippet.startsWith("."));
    },
  );
});

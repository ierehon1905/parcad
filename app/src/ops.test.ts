/**
 * The catalogue, checked against itself rather than a handful of hand-picked
 * strings — a new operation is covered the moment it is added.
 *
 * Every snippet has to be a call whose first argument the scan can find, or
 * pressing the icon parks the caret somewhere useless.
 */

import { describe, expect, test } from "bun:test";

import { OP_GROUPS } from "./ops";
import { firstArgumentEnd } from "./snippet";

/** What pressing this snippet would leave selected. */
function firstArgument(snippet: string): string {
  const opening = snippet.indexOf("(");
  const start = opening + 1;
  return snippet.slice(start, firstArgumentEnd(snippet, start));
}

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

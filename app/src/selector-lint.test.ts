/**
 * The linter's job is the *range*, not the message — the message is already
 * pinned by the shared corpus. These cases mark the expected underline with
 * `~` under the source so a wrong offset is visible rather than arithmetic.
 */

import { expect, test } from "bun:test";
import { javascript } from "@codemirror/lang-javascript";
import { EditorState } from "@codemirror/state";

import { selectorDiagnostics } from "./selector-lint";

function diagnose(doc: string) {
  return selectorDiagnostics(EditorState.create({ doc, extensions: [javascript()] }));
}

/** Render the underlined range the way the cases below write it. */
function underline(from: number, to: number) {
  return " ".repeat(from) + "~".repeat(Math.max(1, to - from));
}

test("blames the offending term, not the whole call", () => {
  const doc = `return box(10, 10, 10).edges(">Z and >QQ").fillet(2)`;
  const [diagnostic, ...rest] = diagnose(doc);
  expect(rest).toEqual([]);
  expect(underline(diagnostic.from, diagnostic.to)).toBe(
    //     return box(10, 10, 10).edges(">Z and >QQ").fillet(2)
    `                                     ~~~`,
  );
  expect(diagnostic.message).toBe(
    'invalid edge-selector term ">QQ"; expected >X, <Y, or |Z (joined with `and`)',
  );
});

test("rejects a parallel term for vertices, at the term", () => {
  const doc = `return box(10, 10, 10).vertices("|X and >Z").chamfer(1)`;
  const [diagnostic, ...rest] = diagnose(doc);
  expect(rest).toEqual([]);
  expect(underline(diagnostic.from, diagnostic.to)).toBe(
    //     return box(10, 10, 10).vertices("|X and >Z").chamfer(1)
    `                                 ~~`,
  );
  expect(diagnostic.message).toBe(
    "vertex selectors use only >X or <X extrema; |X applies to edges",
  );
});

test("accepts what the kernel accepts", () => {
  expect(diagnose(`box(1, 1, 1).edges(">Z and >Y and |X").fillet(1)`)).toEqual([]);
  expect(diagnose(`box(1, 1, 1).vertices(">X and >Y and >Z").chamfer(1)`)).toEqual([]);
  expect(diagnose(`box(1, 1, 1).edges("  <y  ").fillet(1)`)).toEqual([]);
});

test("reports each bad selector in a chain", () => {
  expect(diagnose(`box(1, 1, 1).edges(">Q").fillet(1).edges("+Z").chamfer(1)`)).toHaveLength(2);
});

test("leaves alone what it cannot check", () => {
  // An object query has its own validation in the DSL.
  expect(diagnose(`box(1, 1, 1).edges({ curve: "circle" }).fillet(1)`)).toEqual([]);
  // A computed selector is only knowable at run time.
  expect(diagnose(`box(1, 1, 1).edges(axis + "Z").fillet(1)`)).toEqual([]);
  // A property named `edges` is not a selector call.
  expect(diagnose(`const opts = { edges: ">Q" }`)).toEqual([]);
  // An escape would shift the reported offsets.
  expect(diagnose(`box(1, 1, 1).edges(">\\u005AQ").fillet(1)`)).toEqual([]);
});

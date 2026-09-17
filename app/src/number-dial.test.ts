/**
 * What the dial is allowed to grab, and what it writes back.
 *
 * The two questions that can go wrong silently: whether a click lands on the
 * literal the author meant — a minus sign, a caret on an edge, a digit inside a
 * selector string — and whether the value that comes back is the one they can
 * read. Everything else about the extension is CodeMirror's.
 */

import { expect, test } from "bun:test";
import { javascript } from "@codemirror/lang-javascript";
import { EditorState } from "@codemirror/state";

import { numberAt, stepFor, stepped } from "./number-dial";

/** The literal the dial would engage at `pos`, as text. */
function at(doc: string, pos: number): string | undefined {
  const state = EditorState.create({ doc, extensions: [javascript()] });
  const range = numberAt(state, pos);
  return range && doc.slice(range.from, range.to);
}

test("engages from either edge of a literal and from inside it", () => {
  const doc = `box(1.25, 2)`;
  //           0123456789
  expect(at(doc, 4)).toBe("1.25");
  expect(at(doc, 6)).toBe("1.25");
  expect(at(doc, 8)).toBe("1.25");
  expect(at(doc, 3)).toBeUndefined();
});

test("takes a unary minus with the digits", () => {
  const doc = `at(-5, 0)`;
  expect(at(doc, 5)).toBe("-5");
  // Subtraction is not a sign: the dial gets the operand, and 8 - 3 dialled up
  // is 8 - 4 rather than 9.
  expect(at(`8 - 3`, 5)).toBe("3");
});

test("leaves the digits inside a string alone", () => {
  expect(at(`edges("longerThan: 3")`, 20)).toBeUndefined();
  expect(at(`// 12 mm`, 4)).toBeUndefined();
});

test("keeps the precision the literal was written with", () => {
  expect(stepped("1.2", 0.1)).toBe("1.3");
  expect(stepped("1.2", 1)).toBe("2.2");
  expect(stepped("2.50", 1)).toBe("3.50");
  expect(stepped("3", 1)).toBe("4");
  expect(stepped("3", 0.1)).toBe("3.1");
  expect(stepped("-1", 1)).toBe("0");
});

test("refuses a literal arithmetic would renotate", () => {
  expect(stepped("0x10", 1)).toBeUndefined();
  expect(stepped("1e3", 1)).toBeUndefined();
  expect(stepped("1_000", 1)).toBeUndefined();
});

test("steps a fraction by a fraction", () => {
  const plain = { shiftKey: false, altKey: false };
  expect(stepFor("12", plain)).toBe(1);
  expect(stepFor("0.4", plain)).toBe(0.1);
  expect(stepFor("-0.4", plain)).toBe(0.1);
  expect(stepFor("12", { shiftKey: true, altKey: false })).toBe(10);
  expect(stepFor("12", { shiftKey: false, altKey: true })).toBe(0.1);
});

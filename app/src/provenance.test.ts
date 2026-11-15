import { describe, expect, test } from "bun:test";

import { Provenance, stackLines } from "./provenance";

// Three keys from one helper, as a script writes them:
//
//   2  const key = (x) => box(20, 20, 3).at(x, 0, 0);   // nodes 0..5, two per key
//   5  const keys = [-24, 0, 24].map(key);               // the call site of all three
//   6  const deck = box(80, 30, 2);                      // node 6
//   7  return deck.union(...keys);                        // node 7
//
// Each key's box (stack 2 → 5) and its `.at` (2 → 5); the deck on 6; the union on 7.
const frames = [
  [2, 5], [2, 5], // key 0: box, at
  [2, 5], [2, 5], // key 1
  [2, 5], [2, 5], // key 2
  [6],
  [7],
];
// Faces 0..2: the tops of the three keys; 3: the deck's top, which the union merged a key into.
const chains = [
  [[0, 1, 7]],
  [[2, 3, 7]],
  [[4, 5, 7]],
  [[6, 7], [0, 1, 7]],
];

describe("Provenance", () => {
  const made = new Provenance(frames, chains);

  test("a line that is only ever called from one place is where the story is told", () => {
    // Line 2 is called from line 5 alone, so it is not a helper here: the box is told on line 2.
    expect(made.rowsOf(1)).toEqual([
      { line: 2, nodes: [2, 3] },
      { line: 7, nodes: [7] },
    ]);
  });

  test("a helper called from two places is skipped for its caller", () => {
    const twice = new Provenance(
      [[2, 5], [2, 6], [7]],
      [[[0, 2]], [[1, 2]]],
    );
    expect(twice.rowsOf(0).map((r) => r.line)).toEqual([5, 7]);
    expect(twice.rowsOf(1).map((r) => r.line)).toEqual([6, 7]);
  });

  test("a feature is what its line made, narrowed to the use the face came through", () => {
    // All three keys are made on line 2; the face is on the first key and so is its feature.
    expect([...made.featureFaces(0)].sort()).toEqual([0, 3]);
    expect([...made.featureFaces(2)]).toEqual([2]);
  });

  test("a line made the faces its nodes made first, and shaped the ones that passed it", () => {
    const deck = made.facesOfLine(6);
    expect([...deck.made]).toEqual([3]);
    expect([...deck.shaped]).toEqual([]);
    const union = made.facesOfLine(7);
    expect([...union.made]).toEqual([]);
    expect([...union.shaped].sort()).toEqual([0, 1, 2, 3]);
    // A face of two makers is shaped by the line of the one it is not first told by.
    expect([...made.facesOfLine(2).made].sort()).toEqual([0, 1, 2]);
    expect([...made.facesOfLine(2).shaped]).toEqual([3]);
  });

  test("a step is every face its nodes made or passed on", () => {
    expect([...made.facesOfRow({ line: 7, nodes: [7] })].sort()).toEqual([0, 1, 2, 3]);
  });

  test("a face nothing in the script made has no story", () => {
    expect(new Provenance([[]], [[[0]]]).rowsOf(0)).toEqual([]);
    expect(made.rowsOf(99)).toEqual([]);
  });
});

describe("stackLines", () => {
  test("reads every frame of the script, innermost first, in V8's form and JavaScriptCore's", () => {
    const v8 = [
      "Error",
      "    at new Shape (http://localhost/src/dsl.ts:120:20)",
      "    at key (parcad-editor.js:5:30)",
      "    at Array.map (<anonymous>)",
      "    at eval (parcad-editor.js:8:28)",
    ].join("\n");
    expect(stackLines(v8, "parcad-editor.js", 3)).toEqual([2, 5]);
    const jsc = "Shape@http://localhost/src/dsl.ts:120:20\nkey@parcad-editor.js:5:30\nmap@[native code]\nanonymous@parcad-editor.js:8:28";
    expect(stackLines(jsc, "parcad-editor.js", 3)).toEqual([2, 5]);
  });

  test("drops what the compiler wrote above the script, and anything without a stack", () => {
    expect(stackLines("at x (parcad-editor.js:2:1)", "parcad-editor.js", 3)).toEqual([]);
    expect(stackLines(undefined, "parcad-editor.js", 3)).toEqual([]);
    expect(stackLines("at x (parcad-editor.js:9:1)", "parcad-editor.js", undefined)).toEqual([]);
  });
});

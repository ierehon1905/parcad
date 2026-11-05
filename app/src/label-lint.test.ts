/**
 * What is a label is the DSL's finding, tested there; this holds the range
 * the editor underlines and that an object key draws nothing.
 */

import { expect, test } from "bun:test";
import { javascript } from "@codemirror/lang-javascript";
import { EditorState } from "@codemirror/state";

import { labelDiagnostics } from "./label-lint";

function diagnose(doc: string) {
  return labelDiagnostics(EditorState.create({ doc, extensions: [javascript()] }));
}

test("underlines the word written as a label, with the build's own refusal", () => {
  const doc = "const part = box(10, 10, 10);\nchecks: [{ size: { is: [10, 10, 10] } }];\nreturn part;";
  const [diagnostic, ...rest] = diagnose(doc);
  expect(rest).toEqual([]);
  expect(doc.slice(diagnostic.from, diagnostic.to)).toBe("checks");
  expect(diagnostic.severity).toBe("error");
  expect(diagnostic.message).toStartWith("`checks:` at line 2 is a JavaScript label");
});

test("draws nothing on the returned object's key", () => {
  expect(diagnose("return {\n  part: box(10, 10, 10),\n  checks: [{ size: { is: [10, 10, 10] } }],\n};")).toEqual([]);
});

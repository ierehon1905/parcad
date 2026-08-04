import { expect, test } from "bun:test";
import { EditorState } from "@codemirror/state";
import { javascript } from "@codemirror/lang-javascript";
import * as dsl from "../src/dsl.ts";
import { instrumentTreatmentCalls, treatmentAtCursor, treatmentCallRange } from "../src/source-link.ts";

test("a treatment's final edge can highlight its complete authored chain", () => {
  const source = `const body = box(20, 10, 8);
return body
  .edges({ curve: "circle", role: "hole" })
  .expect({ count: 4 })
  .fillet(1)
  .tag("top_hole_rims");`;
  const api = { ...dsl };
  const names = Object.keys(api);
  const result = new Function(...names, `${source}\n//# sourceURL=parcad-editor.js`)(
    ...names.map((name) => api[name]),
  );
  const treatments = [];
  dsl.build(result, treatments);

  expect(treatments).toHaveLength(1);
  expect(treatments[0].source).toMatchObject({ method: "fillet", line: 5 });

  const state = EditorState.create({ doc: source, extensions: [javascript()] });
  const range = treatmentCallRange(state, source, treatments[0].source);
  expect(source.slice(range.from, range.to)).toBe(
    'body\n  .edges({ curve: "circle", role: "hole" })\n  .expect({ count: 4 })\n  .fillet(1)',
  );
});

test("syntax-derived treatment locations do not depend on Error.stack", () => {
  const source = `const body = box(20, 10, 8);
return body
  .edges({ curve: "circle", role: "hole" })
  .fillet(1)
  .tag("top_hole_rims");`;
  const state = EditorState.create({ doc: source, extensions: [javascript()] });
  const api = { ...dsl };
  const names = Object.keys(api);
  const executable = instrumentTreatmentCalls(state, source);
  const result = new Function(...names, executable)(...names.map((name) => api[name]));
  const treatments = [];
  dsl.build(result, treatments);

  expect(treatments).toHaveLength(1);
  expect(treatments[0].source).toEqual({ method: "fillet", line: 4, column: 4 });
});

test("instrumentation keeps each location in a chain of treatments", () => {
  const source = `const body = box(20, 10, 8);
return body
  .edges(">Z and >Y and |X")
  .fillet(1)
  .edges("<Z and >Y and |X")
  .chamfer(1);`;
  const state = EditorState.create({ doc: source, extensions: [javascript()] });
  const api = { ...dsl };
  const names = Object.keys(api);
  const result = new Function(...names, instrumentTreatmentCalls(state, source))(
    ...names.map((name) => api[name]),
  );
  const treatments = [];
  dsl.build(result, treatments);

  expect(treatments.map((treatment) => treatment.source)).toEqual([
    { method: "fillet", line: 4, column: 4 },
    { method: "chamfer", line: 6, column: 4 },
  ]);
});

test("a selector or expectation cursor previews its owning treatment", () => {
  const source = `const body = box(20, 10, 8);
return body
  .edges(">Z and >Y and |X")
  .expect({ count: 1 })
  .fillet(1)
  .edges("<Z and >Y and |X")
  .expect({ count: 1 })
  .chamfer(1);`;
  const state = EditorState.create({ doc: source, extensions: [javascript()] });
  const api = { ...dsl };
  const names = Object.keys(api);
  const result = new Function(...names, instrumentTreatmentCalls(state, source))(
    ...names.map((name) => api[name]),
  );
  const treatments = [];
  dsl.build(result, treatments);

  expect(treatmentAtCursor(state, source, treatments, source.indexOf(">Z"))?.source?.method).toBe("fillet");
  expect(treatmentAtCursor(state, source, treatments, source.indexOf("count"))?.source?.method).toBe("fillet");
  expect(treatmentAtCursor(state, source, treatments, source.lastIndexOf("count"))?.source?.method).toBe("chamfer");
});

test("a vertex selector cursor previews its corner treatment", () => {
  const source = `const body = box(20, 10, 8);
return body
  .vertices(">X and >Y and >Z")
  .expect({ count: 1 })
  .fillet(1);`;
  const state = EditorState.create({ doc: source, extensions: [javascript()] });
  const api = { ...dsl };
  const names = Object.keys(api);
  const result = new Function(...names, instrumentTreatmentCalls(state, source))(
    ...names.map((name) => api[name]),
  );
  const treatments = [];
  dsl.build(result, treatments);

  expect(treatmentAtCursor(state, source, treatments, source.indexOf(">X"))?.source?.method).toBe("fillet");
});

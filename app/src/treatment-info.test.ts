import { expect, test } from "bun:test";

import {
  treatmentActions,
  treatmentRows,
  type ResolvedTarget,
  type TreatmentNode,
} from "./treatment-info";

const FILLET: TreatmentNode = {
  op: "fillet",
  radius: 2,
  child: 0,
  selector: ">Z and |X",
} as TreatmentNode;

const target = (over: Partial<ResolvedTarget> = {}): ResolvedTarget => ({
  node: 4,
  edges: [1, 2, 3, 4],
  ...over,
});

/** Rows as `label: value`, which is how they read in the tooltip. */
const rows = (...args: Parameters<typeof treatmentRows>) =>
  treatmentRows(...args).map((row) => `${row.label}: ${row.value}`);

test("describes a treatment before the kernel has answered", () => {
  expect(rows(FILLET, undefined, true)).toEqual([
    "radius: 2 mm",
    'selector: ">Z and |X"',
    "resolves: …",
  ]);
  // In a plain browser nothing will ever resolve; do not promise otherwise.
  expect(rows(FILLET)).toEqual(["radius: 2 mm", 'selector: ">Z and |X"']);
});

test("reports the resolved count, and whether expect still holds", () => {
  expect(rows(FILLET, target())).toContain("resolves: 4 edges");

  const pinned = { ...FILLET, expect: { count: 4 } };
  expect(treatmentRows(pinned, target()).at(-1)).toEqual({
    label: "expect",
    value: "4 — holds",
    tone: "ok",
  });

  const drifted = { ...FILLET, expect: { count: 6 } };
  expect(treatmentRows(drifted, target()).at(-1)).toEqual({
    label: "expect",
    value: "6, but 4 edges match",
    tone: "warn",
  });
});

test("counts corners, not their incident edges, for a vertex treatment", () => {
  const corner: TreatmentNode = { op: "chamfer", distance: 1, vertices: ">X and >Y and >Z" };
  expect(rows(corner, target({ vertices: [1], edges: [1, 2, 3] }))).toEqual([
    "distance: 1 mm",
    'corners: ">X and >Y and >Z"',
    "resolves: 1 corner",
  ]);
});

test("offers to pin the count it just measured", () => {
  const call = `.edges(">Z and |X").fillet(2)`;
  const [action, ...rest] = treatmentActions(FILLET, call, 100, target());
  expect(rest).toEqual([]);
  expect(action.label).toBe("pin expect({ count: 4 })");
  // Inserted between the selection and the treatment it feeds.
  expect(action.edit).toEqual({ from: 100 + 19, to: 100 + 19, insert: ".expect({ count: 4 })" });
  expect(applied(call, 100, action.edit)).toBe(
    `.edges(">Z and |X").expect({ count: 4 }).fillet(2)`,
  );
});

test("does not offer a count that is already pinned", () => {
  const pinned = { ...FILLET, expect: { count: 4 } };
  const call = `.edges(">Z and |X").expect({ count: 4 }).fillet(2)`;
  expect(treatmentActions(pinned, call, 0, target())).toEqual([]);
});

test("offers a provenance selector only when a tag matches exactly", () => {
  const call = `.edges(">Z and |X").fillet(2)`;
  const withTag = treatmentActions(FILLET, call, 0, target({ provenance: ["mount_holes"] }));
  const swap = withTag.find((action) => action.label.startsWith("select by"))!;
  expect(swap.label).toBe('select by generatedBy: "mount_holes"');
  expect(applied(call, 0, swap.edit)).toBe(
    `.edges({ generatedBy: "mount_holes" }).fillet(2)`,
  );

  // No exact tag: the kernel reported none, so nothing is suggested.
  expect(treatmentActions(FILLET, call, 0, target({ provenance: [] }))).toHaveLength(1);
});

test("offers nothing until the target is known", () => {
  expect(treatmentActions(FILLET, `.edges(">Z and |X").fillet(2)`, 0, undefined)).toEqual([]);
});

function applied(call: string, callFrom: number, edit: { from: number; to: number; insert: string }) {
  return (
    call.slice(0, edit.from - callFrom) + edit.insert + call.slice(edit.to - callFrom)
  );
}

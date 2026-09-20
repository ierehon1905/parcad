/**
 * The `checks` key of a returned object: data beside the bodies, validated
 * where it is written. The host's `envelope.rs` refuses the same shapes in the
 * same words; `crates/parcad-core/src/checks.rs` carries the synonym table.
 */
import { describe, expect, test } from "bun:test";
import { box, build, cylinder } from "./dsl";

function part(checks: unknown) {
  const top = box(60, 40, 3).tag("top");
  const stack = cylinder(12, 20).at(0, 0, 13.13).tag("stack");
  return build({ top, stack, checks } as Parameters<typeof build>[0]);
}

describe("checks in the returned object", () => {
  test("a check is stamped on the graph beside requires, not as a body", () => {
    const doc = part([{ clear: ["top", "stack"], atLeast: 0.2, why: "coins" }]);
    expect(doc.checks).toEqual([{ clear: ["top", "stack"], atLeast: 0.2, why: "coins" }]);
    expect(doc.requires?.map((r) => r.feature)).toEqual(["part-checks"]);
    const bodies = doc.nodes[doc.root] as { op: string; bodies: { name: string }[] };
    expect(bodies.bodies.map((b) => b.name)).toEqual(["top", "stack"]);
  });

  test("an unknown key is refused with the key that was meant", () => {
    expect(() => part([{ clear: ["top", "stack"], clearance: 0.2 }])).toThrow(
      'check 1 has no key "clearance" (write atLeast instead). A check\'s keys are clear, interferes, touching, wall, size, standsOn, bodies, watertight, atLeast, ignore, on and why.',
    );
    expect(() => part([{ thickness: { min: 1 } }])).toThrow('check 1 has no key "thickness" (write wall instead)');
    expect(() => part([{ clear: ["top", "stack"] }, { why: "nothing" }])).toThrow("check 2 names nothing to check");
    expect(() => part([{ clear: ["top", "stack"], wall: { min: 1 } }])).toThrow("check 1 carries clear and wall at once");
    expect(() => part([{ clear: "top" }])).toThrow('check 1: clear is a pair of body names, e.g. clear: ["top", "stacks"], not "top".');
  });

  test("a check names a body the object returns", () => {
    expect(() => part([{ interferes: ["top", "stacks"] }])).toThrow(
      'check 1 names a body "stacks" the returned object does not have; its bodies are "top" and "stack"',
    );
  });

  test("a body cannot be named checks, and checks cannot be the only key", () => {
    expect(() => part(box(1, 1, 1))).toThrow('a body cannot be named "checks"');
    expect(() => build({ checks: [{ watertight: true }] } as Parameters<typeof build>[0])).toThrow(
      "the script returned checks and no bodies",
    );
    expect(() => part({ clear: ["top", "stack"] })).toThrow("checks is a plain object, not a list of checks");
  });
});

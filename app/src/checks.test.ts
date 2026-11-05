/**
 * The `checks` key of a returned object: data beside the bodies, validated
 * where it is written. The host's `envelope.rs` refuses the same shapes in the
 * same words; `crates/parcad-core/src/checks.rs` carries the synonym table.
 */
import { describe, expect, test } from "bun:test";
import { box, build, cylinder, type Check } from "./dsl";

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
      'check 1 has no key "clearance" (write atLeast instead). A check\'s keys are clear, interferes, touching, turns, slides, wall, size, holes, feature, standsOn, bodies, watertight, clearOf, catches, atLeast, deeperThan, contactAtLeast, ignore, on and why.',
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

  test("a motion check names its body, its axis and its range, and is stamped as a feature", () => {
    const fold: Check = { turns: { body: "stack", about: "x", at: [0, 0, 3], degrees: [0, -90] }, clearOf: ["top"], atLeast: 0.3 };
    const doc = part([fold]);
    expect(doc.checks).toEqual([fold]);
    expect(doc.requires?.map((r) => r.feature)).toEqual(["part-checks", "motion-checks"]);
    const pull: Check = { slides: { body: "stack", along: [0, 0, 1], mm: [0, 20] }, catches: "top", deeperThan: 0.2 };
    expect(part([pull]).checks).toEqual([pull]);
    expect(() => part([{ rotates: { body: "stack" } }])).toThrow('check 1 has no key "rotates" (write turns instead)');
    expect(() => part([{ turns: "stack", about: "x" }])).toThrow(
      'check 1 has no key "about". A check\'s keys are',
    );
    expect(() => part([{ turns: { body: "stack", axis: "x", at: [0, 0, 3], degrees: [0, -90] } }])).toThrow(
      'check 1: turns has no key "axis"; its keys are body, about, at and degrees',
    );
    expect(() => part([{ turns: { body: "stack", about: "x", at: [0, 0], degrees: [0, -90] } }])).toThrow(
      "check 1: turns.at is a point on the axis, [x, y, z] in mm",
    );
    expect(() => part([{ turns: { body: "stack", about: "x", at: [0, 0, 3], degrees: 90 } }])).toThrow(
      "check 1: turns.degrees is [from, to] in degrees, from the body as drawn at 0, not 90.",
    );
    expect(() => part([{ slides: { body: "stack", along: "y", mm: [0, 20] }, clearOf: 3 }])).toThrow(
      'check 1: clearOf is a body name or a list of them, e.g. clearOf: ["base"], not 3.',
    );
    expect(() => part([{ slides: { body: "stack", along: "y", mm: [0, 20] }, catches: ["top"] }])).toThrow(
      "check 1: catches is the name of the one body the moving one must bite into",
    );
    expect(() => part([{ turns: { body: "lid", about: "x", at: [0, 0, 3], degrees: [0, -90] } }])).toThrow(
      'check 1 names a body "lid" the returned object does not have; its bodies are "top" and "stack"',
    );
    expect(() => part([{ turns: { body: "stack", about: "x", at: [0, 0, 3], degrees: [0, -90] }, clearOf: "base" }])).toThrow(
      'check 1 names a body "base" the returned object does not have',
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

describe("reference bodies", () => {
  test("the mark lands on the body the object names, and is stamped as a feature", () => {
    const tray = box(60, 40, 3).tag("tray");
    const stack = cylinder(12, 20).at(0, 0, 13.5).reference();
    const doc = build({ tray, stack });
    const bodies = (doc.nodes[doc.root] as { bodies: { name: string; child: number; reference?: boolean }[] }).bodies;
    expect(bodies).toEqual([
      { name: "tray", child: 0 },
      { name: "stack", child: 2, reference: true },
    ]);
    expect(doc.requires?.map((r) => r.feature)).toEqual(["reference-bodies"]);
  });

  test("a reference is never part of the part", () => {
    const coin = cylinder(12, 2).reference();
    expect(() => build({ tray: box(60, 40, 3), stack: coin.at(0, 0, 10) })).toThrow(
      "a shape marked .reference() was used to build another shape",
    );
    expect(() => build(box(1, 1, 1).reference())).toThrow("the script returned only a reference");
    expect(() => build({ a: box(1, 1, 1).reference(), b: box(2, 2, 2).reference() })).toThrow(
      "every body is a reference",
    );
  });
});

describe("print orientation", () => {
  test("printedUp lands on the body the object names, as a unit direction, and is stamped as a feature", () => {
    const base = box(40, 40, 20).at(0, 0, 10);
    const lid = box(40, 40, 3).at(0, 0, 21.5).printedUp("-z");
    const doc = build({ base, lid });
    const bodies = (doc.nodes[doc.root] as { bodies: { name: string; child: number; printed_up?: { x: number; y: number; z: number } }[] }).bodies;
    expect(bodies[0].printed_up).toBeUndefined();
    expect(bodies[1].printed_up).toEqual({ x: 0, y: 0, z: -1 });
    expect(doc.requires?.map((r) => r.feature)).toEqual(["print-orientation"]);
    const tilted = build({ base, lid: box(40, 40, 3).at(0, 0, 21.5).printedUp([3, 0, 4]) });
    expect((tilted.nodes[tilted.root] as { bodies: { printed_up?: { x: number; y: number; z: number } }[] }).bodies[1].printed_up).toEqual({ x: 0.6, y: 0, z: 0.8 });
  });

  test("a reference, a lone shape and a bad axis are refused by name", () => {
    expect(() => build({ base: box(1, 1, 1), stack: cylinder(1, 1).reference().printedUp("-z") })).toThrow("is a reference and has a print orientation");
    expect(() => build(box(1, 1, 1).printedUp("-z"))).toThrow("return it as a named body");
    expect(() => box(1, 1, 1).printedUp("up" as never)).toThrow('"+z" (as drawn)');
    expect(() => box(1, 1, 1).printedUp([0, 0, 0])).toThrow("points nowhere");
  });
});

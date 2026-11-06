/**
 * A `mechanism` beside the bodies: validated where it is written, every law
 * sampled into knots before the graph leaves the script. The contract with
 * the Rust side is docs in the side-session notes, mechanism-contract.md.
 */
import { describe, expect, test } from "bun:test";
import { box, build } from "./dsl";

const PIN: [number, number, number] = [0, 0, 40];
const push = (t: number) => 30 * Math.sin((18 * t * Math.PI) / 180);

function part(mechanism: unknown, checks?: unknown) {
  const base = box(90, 30, 4).at(0, 0, 2);
  const lever = box(6, 10, 30).at(0, 0, 25);
  const slider = box(20, 10, 8).at(13.1, 0, 14);
  return build({ base, lever, slider, mechanism, ...(checks !== undefined && { checks }) } as unknown as Parameters<typeof build>[0]);
}

describe("a mechanism in the returned object", () => {
  test("is stamped on the graph as joints with knots, not as a body", () => {
    const doc = part(
      { lever: { turns: { about: "y", at: PIN, degrees: [0, -18] } }, slider: { slides: { along: "x", mm: push } } },
      [{ moves: "all", atLeast: 0.3 }, { pushes: ["lever", "slider"], within: 0.2 }],
    );
    const bodies = doc.nodes[doc.root] as { bodies: { name: string }[] };
    expect(bodies.bodies.map((b) => b.name)).toEqual(["base", "lever", "slider"]);
    expect(doc.mechanism?.drive).toEqual([0, 1]);
    const [lever, slider] = doc.mechanism!.joints;
    expect(lever).toEqual({ body: "lever", turns: { about: "y", at: PIN }, knots: [[0, 0], [1, -18]] });
    expect(slider.slides).toEqual({ along: "x" });
    expect(slider.knots![0]).toEqual([0, 0]);
    expect(slider.knots!.at(-1)![0]).toBe(1);
    expect(doc.requires?.map((r) => r.feature)).toContain("mechanism");
  });

  test("samples a function until straight pieces stay within 0.005 of it", () => {
    const knots = part({ slider: { slides: { along: "x", mm: push } } }).mechanism!.joints[0].knots!;
    for (let i = 1; i < knots.length; i++) expect(knots[i][0]).toBeGreaterThan(knots[i - 1][0]);
    for (let i = 0; i + 1 < knots.length; i++) {
      const [t0, v0] = knots[i];
      const [t1, v1] = knots[i + 1];
      for (const f of [0.1, 0.33, 0.5, 0.9]) {
        expect(Math.abs(push(t0 + (t1 - t0) * f) - (v0 + (v1 - v0) * f))).toBeLessThan(0.005);
      }
    }
    const ranged = part({ drive: [0, 28], slider: { slides: { along: "x", mm: [2, 7] } } }).mechanism!;
    expect(ranged.joints[0].knots).toEqual([[0, 2], [28, 7]]);
  });

  test("a body rides another with on, and a joint may sit on one", () => {
    const doc = part({ lever: { turns: { about: "y", at: PIN, degrees: [0, -18] } }, slider: { on: "lever" } });
    expect(doc.mechanism!.joints[1]).toEqual({ body: "slider", on: "lever" });
  });

  test("a mechanism that cannot be read is refused, naming what to write", () => {
    expect(() => part({ lever: { rotates: { about: "y", at: PIN, degrees: [0, -18] } } })).toThrow(
      'mechanism.lever has no key "rotates" (write turns instead)',
    );
    expect(() => part({ handle: { slides: { along: "x", mm: [0, 1] } } })).toThrow(
      'mechanism names a body "handle" the returned object does not have; its bodies are "base", "lever" and "slider"',
    );
    expect(() => part({ lever: { turns: { about: "y", at: PIN, degrees: [0, -18] }, slides: { along: "x", mm: [0, 1] } } })).toThrow(
      "mechanism.lever both turns and slides",
    );
    expect(() => part({ lever: { on: "lever" } })).toThrow("mechanism.lever.on names the body that carries this one");
    expect(() => part({ lever: {} })).toThrow("mechanism.lever says nothing about how it moves");
    expect(() => part({ drive: [1, 0], lever: { turns: { about: "y", at: PIN, degrees: [0, -18] } } })).toThrow(
      "mechanism.drive is the drive's range, low to high",
    );
    expect(() => part({ lever: { slides: { along: "x", mm: [3, 3] } } })).toThrow("mechanism.lever never moves: its law is 3 mm all along the drive");
    expect(() => part({ lever: { slides: { along: "x", mm: () => Number.NaN } } })).toThrow(
      "mechanism.lever.slides.mm gave NaN at drive 0",
    );
  });

  test("checks over a mechanism need one, and read their own shapes", () => {
    const base = box(90, 30, 4);
    const lever = box(6, 10, 30).at(0, 0, 25);
    expect(() => build({ base, lever, checks: [{ moves: "all", atLeast: 0.3 }] } as Parameters<typeof build>[0])).toThrow(
      "check 1 holds over a mechanism and the part has none",
    );
    const lever2 = { lever: { turns: { about: "y", at: PIN, degrees: [0, -18] } } };
    expect(() => part(lever2, [{ pushes: ["lever", "slider"] }])).toThrow("check 1: a pushes check says how far the two may open apart or overlap");
    expect(() => part(lever2, [{ moves: "every", atLeast: 0.3 }])).toThrow('check 1: moves is "all" or a list of the bodies to watch');
    expect(() => part(lever2, [{ push: ["lever", "slider"], within: 0.1 }])).toThrow('check 1 has no key "push" (write pushes instead)');
  });
});

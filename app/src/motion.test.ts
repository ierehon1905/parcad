import { describe, expect, test } from "bun:test";

import { bodyPoses, mechanismOf, motionsOf, poseAt, travelPoses, valueAt, type Mechanism } from "./motion";
import type { MotionReport } from "./state";

/** The lid's three checks as eval/scripts/hinged-box.js's build reports them. */
const hinge = { axis: [1, 0, 0] as [number, number, number], at: [0, -28, 16.2] as [number, number, number] };
const lid: MotionReport[] = [
  {
    check: "turns lid about x at (0, -28, 16.2) 0° to -170° clearOf base atLeast 0.3",
    verdict: "passed",
    body: "lid",
    turn: { ...hinge, degrees: [0, -170] },
    at_degrees: -170,
    // The kernel's own transform at that pose, from the reply.
    worst_pose: [
      1, 0, 0, 0, 0, -0.984807753012208, 0.17364817766693028, -58.387717562546094, 0, -0.17364817766693028,
      -0.984807753012208, 27.291736624123722, 0, 0, 0, 1,
    ],
    measurements: 414566,
  },
  {
    check: "turns lid about x at (0, -28, 16.2) -170° to -180° catches base deeperThan 0.2",
    verdict: "passed",
    body: "lid",
    turn: { ...hinge, degrees: [-170, -180] },
    at_degrees: -179.258,
    measurements: 34,
  },
  {
    check: "turns lid about x at (0, -28, 16.2) 0° to -170° clearOf object atLeast 0.3",
    verdict: "failed",
    body: "lid",
    turn: { ...hinge, degrees: [0, -170] },
    at_degrees: -165.85,
    measurements: 26,
  },
];

describe("motionsOf", () => {
  test("checks on one pin are one motion over every range they name, from the drawn pose", () => {
    const [motion, ...rest] = motionsOf(lid);
    expect(rest).toEqual([]);
    expect(motion.label).toBe("lid");
    expect(motion.moves).toMatchObject([{ body: "lid", kind: "turn", knots: [[0, 0], [1, -180]] }]);
    expect(motion.failsAt[0]).toBeCloseTo(165.85 / 180, 12);
  });

  test("another point on the same pin is the same motion", () => {
    const along = { ...lid[1], turn: { ...hinge, at: [40, -28, 16.2] as [number, number, number], degrees: [-170, -180] as [number, number] } };
    expect(motionsOf([lid[0], along])).toHaveLength(1);
  });

  test("a body that slides two ways has two motions, and a range that leaves the drawn pose still starts there", () => {
    const slide = (direction: [number, number, number], mm: [number, number]): MotionReport => ({
      check: "",
      verdict: "passed",
      body: "drawer",
      slide: { direction, mm },
      measurements: 1,
    });
    const motions = motionsOf([slide([0, 1, 0], [0, 60]), slide([0, 0, 1], [2, 5]), slide([0, 1, 0], [-10, 0])]);
    expect(motions.map((m) => [m.label, m.moves[0].axis, m.moves[0].knots])).toEqual([
      ["drawer y", [0, 1, 0], [[0, -10], [1, 60]]],
      ["drawer z", [0, 0, 1], [[0, 0], [1, 5]]],
    ]);
  });
});

describe("poseAt", () => {
  test("is the kernel's own transform at the pose it measured", () => {
    const [move] = motionsOf(lid)[0].moves;
    const pose = poseAt(move, valueAt(move, 170 / 180));
    lid[0].worst_pose!.forEach((value, i) => expect(pose[i]).toBeCloseTo(value, 12));
  });

  test("leaves the pin where it is", () => {
    const [move] = motionsOf(lid)[0].moves;
    const pose = poseAt(move, -97);
    const [x, y, z] = hinge.at;
    const moved = [0, 1, 2].map((row) => pose[row * 4] * x + pose[row * 4 + 1] * y + pose[row * 4 + 2] * z + pose[row * 4 + 3]);
    moved.forEach((value, i) => expect(value).toBeCloseTo(hinge.at[i], 12));
  });

  test("slides along the direction by the value", () => {
    const [move] = motionsOf([{ check: "", verdict: "passed", body: "drawer", slide: { direction: [0, 1, 0], mm: [0, 60] }, measurements: 1 }])[0].moves;
    expect(poseAt(move, 35)).toEqual([1, 0, 0, 0, 0, 1, 0, 35, 0, 0, 1, 0, 0, 0, 0, 1]);
  });
});

describe("bodyPoses", () => {
  test("a body's motions add up, and a body that does not move has no pose", () => {
    const slide = (direction: [number, number, number]): MotionReport => ({
      check: "",
      verdict: "passed",
      body: "drawer",
      slide: { direction, mm: [0, 60] },
      measurements: 1,
    });
    const poses = bodyPoses(motionsOf([slide([0, 1, 0]), slide([0, 0, 1])]), [0.5, 4 / 60]);
    expect(poses.get("drawer")).toEqual([1, 0, 0, 0, 0, 1, 0, 30, 0, 0, 1, 4, 0, 0, 0, 1]);
    expect(poses.has("frame")).toBe(false);
  });
});

describe("a mechanism", () => {
  const axisZ: [number, number, number] = [0, 0, 1];
  const mesh = mechanismOf({
    mechanism: {
      drive: [0, 28],
      joints: [
        { body: "pinion", kind: "turn", axis: axisZ, at: [0, 0, 0], knots: [[0, 0], [28, 360]] },
        { body: "wheel", kind: "turn", axis: axisZ, at: [50, 0, 0], knots: [[0, 0], [28, -240]] },
        { body: "cap", kind: "rides", parent: "wheel" },
      ],
    },
    checks: {
      verdict: "failed",
      passed: 0,
      mechanism: [{ check: "", verdict: "failed", kind: "moves", at_drive: 7, measurements: 3 }],
    },
  })!;

  test("is one slider over its own drive, named for the first body that moves, marked where a check fails", () => {
    expect(mesh.label).toBe("pinion");
    expect(mesh.drive).toEqual([0, 28]);
    expect(mesh.failsAt).toEqual([7]);
    expect(mesh.moves.map((move) => valueAt(move, 7))).toEqual([90, -60, 0]);
  });

  test("moves every body it names on one drive, a rider with the body that carries it", () => {
    const poses = bodyPoses([mesh], [7]);
    expect(poses.get("pinion")![1]).toBeCloseTo(-1, 12);
    expect(poses.get("wheel")![1]).toBeCloseTo(Math.sin(Math.PI / 3), 12);
    poses.get("cap")!.forEach((v, i) => expect(v).toBeCloseTo(poses.get("wheel")![i], 12));
  });

  test("reads its knots straight between, and holds at the ends", () => {
    const lift = { ...mesh.moves[1], knots: [[0, 0], [10, 2], [20, 10]] as [number, number][] };
    expect([5, 10, 15, -3, 25].map((d) => valueAt(lift, d))).toEqual([1, 2, 6, 0, 10]);
  });

  test("a body carried by a turning one turns with it about the parent's pin, then its own", () => {
    const arm = {
      label: "arm",
      drive: [0, 1] as [number, number],
      moves: [
        { body: "arm", kind: "turn" as const, axis: axisZ, at: [0, 0, 0] as [number, number, number], knots: [[0, 0], [1, 90]] as [number, number][] },
        { body: "hand", kind: "slide" as const, axis: [1, 0, 0] as [number, number, number], at: [0, 0, 0] as [number, number, number], parent: "arm", knots: [[0, 0], [1, 10]] as [number, number][] },
      ],
      failsAt: [],
    };
    // At the end the hand has slid 10 along x and the arm has turned it a quarter: it sits at +y.
    const hand = bodyPoses([arm], [1]).get("hand")!;
    expect(hand[3]).toBeCloseTo(0, 12);
    expect(hand[7]).toBeCloseTo(10, 12);
  });
});

describe("travelPoses", () => {
  const turn = (knots: [number, number][]): Mechanism => ({
    label: "lever",
    drive: [0, 1],
    moves: [{ body: "lever", kind: "turn", axis: [0, 1, 0], at: [0, 0, 0], knots }],
    failsAt: [],
  });
  const zOf = (pose: number[], [x, y, z]: [number, number, number]) => pose[8] * x + pose[9] * y + pose[10] * z + pose[11];

  test("finds the bottom of an arc that dips between two knots", () => {
    const poses = travelPoses([turn([[0, -60], [1, 60]])]).get("lever")!;
    // A point hanging 10 below the pin is lowest at 0°, which neither knot is.
    expect(Math.min(...poses.map((pose) => zOf(pose, [0, 0, -10])))).toBeCloseTo(-10, 9);
    expect(Math.max(...poses.map((pose) => zOf(pose, [0, 0, -10])))).toBeCloseTo(-5, 9);
  });

  test("steps a turn no wider than 2°, and a slide only at its knots", () => {
    expect(travelPoses([turn([[0, 0], [1, -90]])]).get("lever")!.length).toBe(1 + 1 + 45);
    const drawer: Mechanism = {
      label: "drawer",
      drive: [0, 1],
      moves: [{ body: "drawer", kind: "slide", axis: [1, 0, 0], at: [0, 0, 0], knots: [[0, 0], [0.5, 40], [1, 60]] }],
      failsAt: [],
    };
    expect(travelPoses([drawer]).get("drawer")!.map((pose) => pose[3])).toEqual([0, 0, 40, 60]);
  });

  test("sweeps each mechanism with the others where they start", () => {
    const lid = turn([[0, 0], [1, -4]]);
    const drawer: Mechanism = {
      label: "drawer",
      drive: [0, 1],
      moves: [{ body: "drawer", kind: "slide", axis: [1, 0, 0], at: [0, 0, 0], knots: [[0, 0], [1, 60]] }],
      failsAt: [],
    };
    const travel = travelPoses([lid, drawer]);
    expect(travel.get("lever")!.length).toBe(1 + 1 + 2);
    expect(travel.get("drawer")!.map((pose) => pose[3])).toEqual([0, 0, 60]);
  });
});

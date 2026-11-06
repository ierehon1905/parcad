import { describe, expect, test } from "bun:test";

import { bodyPoses, motionsOf, poseAt } from "./motion";
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
    expect(motion).toMatchObject({ body: "lid", kind: "turn", from: 0, to: -180, failsAt: [-165.85] });
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
    expect(motions.map((m) => [m.axis, m.from, m.to])).toEqual([
      [[0, 1, 0], -10, 60],
      [[0, 0, 1], 0, 5],
    ]);
  });
});

describe("poseAt", () => {
  test("is the kernel's own transform at the pose it measured", () => {
    const [motion] = motionsOf(lid);
    const pose = poseAt(motion, -170);
    lid[0].worst_pose!.forEach((value, i) => expect(pose[i]).toBeCloseTo(value, 12));
  });

  test("leaves the pin where it is", () => {
    const [motion] = motionsOf(lid);
    const pose = poseAt(motion, -97);
    const [x, y, z] = hinge.at;
    const moved = [0, 1, 2].map((row) => pose[row * 4] * x + pose[row * 4 + 1] * y + pose[row * 4 + 2] * z + pose[row * 4 + 3]);
    moved.forEach((value, i) => expect(value).toBeCloseTo(hinge.at[i], 12));
  });

  test("slides along the direction by the value", () => {
    const [motion] = motionsOf([{ check: "", verdict: "passed", body: "drawer", slide: { direction: [0, 1, 0], mm: [0, 60] }, measurements: 1 }]);
    expect(poseAt(motion, 35)).toEqual([1, 0, 0, 0, 0, 1, 0, 35, 0, 0, 1, 0, 0, 0, 0, 1]);
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
    const poses = bodyPoses(motionsOf([slide([0, 1, 0]), slide([0, 0, 1])]), [30, 4]);
    expect(poses.get("drawer")).toEqual([1, 0, 0, 0, 0, 1, 0, 30, 0, 0, 1, 4, 0, 0, 0, 1]);
    expect(poses.has("frame")).toBe(false);
  });
});

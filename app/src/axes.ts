/**
 * Which way X, Y and Z point, drawn in the corner of the view.
 *
 * The grid has no axes of its own, while the section switch, the report and
 * every script name them: without this a reader keeps the mapping in their
 * head. An SVG over the canvas, so it costs no draw call and stays out of the
 * thumbnails a canvas copy makes.
 */

import * as THREE from "three";

const SIZE = 76;
const CENTRE = SIZE / 2;
/** Arm length in px, leaving room for a letter past the tip. */
const REACH = 22;

const ARMS: { label: string; axis: THREE.Vector3; colour: string }[] = [
  { label: "X", axis: new THREE.Vector3(1, 0, 0), colour: "var(--color-bad, #ff6b6b)" },
  { label: "Y", axis: new THREE.Vector3(0, 1, 0), colour: "var(--color-good, #7bd88f)" },
  { label: "Z", axis: new THREE.Vector3(0, 0, 1), colour: "var(--color-accent, #6ea8fe)" },
];

const SVG = "http://www.w3.org/2000/svg";

export class AxisTriad {
  private readonly arms: {
    halo: SVGLineElement;
    line: SVGLineElement;
    text: SVGTextElement;
    group: SVGGElement;
    axis: THREE.Vector3;
  }[];
  private readonly svg: SVGSVGElement;
  private readonly inverse = new THREE.Quaternion();
  private readonly turned = new THREE.Vector3();

  constructor(container: HTMLElement) {
    if (getComputedStyle(container).position === "static") container.style.position = "relative";
    this.svg = document.createElementNS(SVG, "svg");
    this.svg.setAttribute("width", String(SIZE));
    this.svg.setAttribute("height", String(SIZE));
    this.svg.setAttribute("aria-hidden", "true");
    Object.assign(this.svg.style, { position: "absolute", right: "14px", bottom: "14px", pointerEvents: "none" });
    this.arms = ARMS.map(({ label, axis, colour }) => {
      const group = document.createElementNS(SVG, "g");
      // A dark halo under each arm and letter, in place of a plate behind them: it reads on the
      // window's light backdrop and vanishes on the chat viewer's dark one (Birman, information garbage).
      const halo = document.createElementNS(SVG, "line");
      halo.setAttribute("x1", String(CENTRE));
      halo.setAttribute("y1", String(CENTRE));
      halo.setAttribute("stroke-width", "5");
      halo.setAttribute("stroke-linecap", "round");
      halo.style.stroke = "rgb(0 0 0 / 0.35)";
      const line = document.createElementNS(SVG, "line");
      line.setAttribute("x1", String(CENTRE));
      line.setAttribute("y1", String(CENTRE));
      line.style.stroke = colour;
      line.setAttribute("stroke-width", "2.5");
      line.setAttribute("stroke-linecap", "round");
      const text = document.createElementNS(SVG, "text");
      text.textContent = label;
      text.style.fill = colour;
      text.setAttribute("font-size", "12");
      text.setAttribute("font-weight", "600");
      text.style.fontFamily = "var(--font-sans, system-ui, sans-serif)";
      Object.assign(text.style, { stroke: "rgb(0 0 0 / 0.45)", strokeWidth: "3px", paintOrder: "stroke" });
      text.setAttribute("text-anchor", "middle");
      text.setAttribute("dominant-baseline", "central");
      group.append(halo, line, text);
      this.svg.append(group);
      return { halo, line, text, group, axis };
    });
    container.append(this.svg);
  }

  /** Point the arms as the camera sees the world's axes; nearer arms are drawn over farther ones. */
  update(camera: THREE.Camera) {
    this.inverse.copy(camera.quaternion).invert();
    const depth: { group: SVGGElement; z: number }[] = [];
    for (const arm of this.arms) {
      const v = this.turned.copy(arm.axis).applyQuaternion(this.inverse);
      const x = CENTRE + v.x * REACH;
      const y = CENTRE - v.y * REACH;
      for (const stroke of [arm.halo, arm.line]) {
        stroke.setAttribute("x2", x.toFixed(1));
        stroke.setAttribute("y2", y.toFixed(1));
      }
      arm.text.setAttribute("x", (CENTRE + v.x * (REACH + 8)).toFixed(1));
      arm.text.setAttribute("y", (CENTRE - v.y * (REACH + 8)).toFixed(1));
      // Depth is the drawing order alone: a fainter far arm fell under the contrast a label needs.
      depth.push({ group: arm.group, z: v.z });
    }
    depth.sort((a, b) => a.z - b.z);
    for (const { group } of depth) this.svg.append(group);
  }
}

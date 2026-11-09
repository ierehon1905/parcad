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
  private readonly arms: { line: SVGLineElement; text: SVGTextElement; group: SVGGElement; axis: THREE.Vector3 }[];
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
    // On the same dark glass as the other panels over the view, which a light backdrop needs.
    const backing = document.createElementNS(SVG, "rect");
    backing.setAttribute("width", String(SIZE));
    backing.setAttribute("height", String(SIZE));
    backing.setAttribute("rx", "4");
    // Through `style`: a custom property is CSS, and an SVG attribute is not read as CSS everywhere.
    Object.assign(backing.style, { fill: "var(--color-glass, #0a0b0d)", fillOpacity: "0.86" });
    this.svg.append(backing);
    this.arms = ARMS.map(({ label, axis, colour }) => {
      const group = document.createElementNS(SVG, "g");
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
      text.setAttribute("text-anchor", "middle");
      text.setAttribute("dominant-baseline", "central");
      group.append(line, text);
      this.svg.append(group);
      return { line, text, group, axis };
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
      arm.line.setAttribute("x2", x.toFixed(1));
      arm.line.setAttribute("y2", y.toFixed(1));
      arm.text.setAttribute("x", (CENTRE + v.x * (REACH + 8)).toFixed(1));
      arm.text.setAttribute("y", (CENTRE - v.y * (REACH + 8)).toFixed(1));
      // An arm pointing away from the viewer is drawn fainter, so the triad reads as 3D.
      arm.group.setAttribute("opacity", v.z < -0.05 ? "0.55" : "1");
      depth.push({ group: arm.group, z: v.z });
    }
    depth.sort((a, b) => a.z - b.z);
    for (const { group } of depth) this.svg.append(group);
  }
}

/**
 * How a part looks in a chat: a dark stage, each body in a colour of its own,
 * a warm key light that casts a soft shadow, a rim light behind, and dark
 * edges a CSS pixel and a quarter wide, so a phone screenshot reads.
 */

import type { SceneLook } from "../viewport";

/** A colour from the stylesheet's @theme block: the chips wear the same ones. */
function token(name: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(`--color-${name}`).trim();
  if (!/^#[0-9a-f]{6}$/i.test(value)) {
    throw new Error(`--color-${name} read as ${value || "nothing"}; the chat viewer's look takes it from @theme in app/src/style.css`);
  }
  return value;
}

/** The colours bodies wear, in the order the part names them. */
export function bodyColors(): string[] {
  return [1, 2, 3, 4, 5, 6, 7, 8].map((n) => token(`body-${n}`));
}

/** `hex` mixed toward white by `amount`, 0 to 1. */
function lighter(hex: string, amount: number): string {
  const channels = [1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16));
  return `#${channels.map((c) => Math.round(c + (255 - c) * amount).toString(16).padStart(2, "0")).join("")}`;
}

export function studio(): SceneLook {
  return {
    // The stage token at the bottom, a touch lighter at the top: depth without a horizon.
    sky: [lighter(token("stage"), 0.07), token("stage")],
    toneMapping: "neutral",
    exposure: 1,
    environment: 0.45,
    lights: [
      { kind: "sun", color: "#fff6ea", intensity: 2.8, from: [-0.5, -0.9, 1.4] },
      { kind: "camera", color: "#c9d6ee", intensity: 0.6, from: [0, 0, 1], toward: [0, 0, -1] },
      { kind: "camera", color: "#ffffff", intensity: 1.4, from: [0, 0.8, -2], toward: [0, 0, -1] },
    ],
    surface: { kind: "standard", roughness: 0.45, metalness: 0 },
    part: token("body-1"),
    // Opaque: at 0.85 two edges meeting at a corner stacked into a darker spot, and every edge read grey.
    edges: { color: "#0e1014", opacity: 1, width: 1.25 },
    shadow: 0.45,
    shadowMap: 1024,
  };
}

/**
 * The blurred surface everything that floats over the viewport is drawn on.
 *
 * Something resting *on* the viewport stays translucent enough to read the part
 * through; something floating above it does not have to.
 */

import type { ComponentChildren } from "preact";

type Variant = "hud" | "target" | "menu" | "tip";

const SURFACE: Record<Variant, string> = {
  /** A chip resting on the viewport: measurements, tools, the report. */
  hud: "rounded-lg border border-line bg-glass/86 backdrop-blur-lg",
  /** The same, marked as the thing the cursor is pointing at. */
  target: "rounded-lg border border-gold-edge bg-glass/86 backdrop-blur-lg",
  /** A panel of choices, floating clear of everything. */
  menu:
    "rounded-xl border border-line bg-glass/95 backdrop-blur-lg " +
    "shadow-[0_18px_44px_rgb(0_0_0/0.55)]",
  /** A tooltip: the same glass, a lighter shadow, never interactive. */
  tip:
    "rounded-lg border border-line bg-glass/95 backdrop-blur-lg " +
    "shadow-[0_10px_28px_rgb(0_0_0/0.5)]",
};

export interface GlassProps {
  variant: Variant;
  /** Where it sits and how big it is. Not what colour it is. */
  layout?: string;
  children?: ComponentChildren;
  id?: string;
  "data-flyout"?: boolean;
  ref?: (element: HTMLDivElement | null) => void;
}

export function Glass({ variant, layout, children, ...rest }: GlassProps) {
  return (
    <div class={`${SURFACE[variant]} ${layout ?? ""}`} {...rest}>
      {children}
    </div>
  );
}

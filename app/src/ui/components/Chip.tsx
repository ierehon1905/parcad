/** A small bordered label beside a part's name: its tags, and what it is. */

import type { ComponentChildren, JSX } from "preact";

const SHAPE = "border rounded-xs px-1";

const COLOURS = {
  default: "border-line",
  /** A part stored as a loose `.js` rather than a `.parcad` folder. */
  note: "text-note border-note-edge",
} as const;

export interface ChipProps extends Omit<JSX.IntrinsicElements["span"], "class"> {
  variant?: keyof typeof COLOURS;
  children?: ComponentChildren;
}

export function Chip({ variant = "default", children, ...rest }: ChipProps) {
  return (
    <span class={`${SHAPE} ${COLOURS[variant]}`} {...rest}>
      {children}
    </span>
  );
}

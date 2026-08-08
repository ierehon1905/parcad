/**
 * The button the dialogs and forms are built from. Shape and colour stay in
 * separate strings — CLAUDE.md, "A variant cannot be the base plus a colour".
 */

import type { JSX } from "preact";

type Variant = "default" | "primary" | "quiet" | "danger";

const SHAPE =
  "px-3 py-1.5 border rounded-md cursor-pointer hover:border-accent " +
  "disabled:opacity-45 disabled:cursor-default";

const COLOURS: Record<Variant, string> = {
  default: "bg-panel-2 text-ink border-line",
  primary: "bg-accent-deep text-ink border-accent-edge",
  /** For a button that must not compete with what it sits beside. */
  quiet: "bg-panel-2 text-ink-dim border-line",
  /** Removal. The only red button in the app. */
  danger: "bg-panel-2 text-bad border-line",
};

export interface ButtonProps extends Omit<JSX.IntrinsicElements["button"], "class"> {
  variant?: Variant;
  /** Layout only — where this one sits. A colour here would land in the hazard
   *  above, since which of two backgrounds wins is not decided here. */
  layout?: string;
}

export function Button({ variant = "default", layout, ...rest }: ButtonProps) {
  return (
    <button type="button" class={`${SHAPE} ${COLOURS[variant]} ${layout ?? ""}`} {...rest} />
  );
}

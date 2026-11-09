/**
 * A button in a bar that is either the chosen one or not.
 *
 * `pressed` reaches the DOM as `aria-pressed` and not only as a colour, so the
 * accent is not the only thing saying which one is chosen.
 */

import type { JSX } from "preact";

type Size = "group" | "segment" | "touch" | "touch-segment";

const SHAPE: Record<Size, string> = {
  group: "flex items-center gap-1.5 px-2 py-1 rounded-md border cursor-pointer",
  /** At least 28 px across and 24 high: X, Y and Z are one letter each, and a target that size is hard to hit. */
  segment: "flex items-center justify-center gap-1 min-w-7 min-h-6 px-1.5 rounded cursor-pointer",
  /** A finger's worth, floating on the view by itself: the chat viewer on a phone. */
  touch: "flex shrink-0 items-center justify-center size-11 rounded-full border cursor-pointer backdrop-blur-lg",
  /** The same height, one of a row in a `TOUCH_TROUGH`. */
  "touch-segment": "flex items-center justify-center h-9 px-3 rounded-full cursor-pointer",
};

const OFF: Record<Size, string> = {
  /** Full ink: dimmed is how a disabled button reads, and these are live. */
  group: "text-ink border-transparent hover:border-line",
  segment: "text-ink-dim hover:text-ink",
  touch: "text-ink border-line bg-glass/86",
  "touch-segment": "text-ink-dim",
};

const ON: Record<Size, string> = {
  group: "text-accent bg-accent-deep/45 border-accent-edge",
  segment: "text-accent bg-accent-deep/70",
  touch: "text-accent border-accent-edge bg-accent-deep/86",
  "touch-segment": "text-ink bg-accent-deep",
};

export interface ToggleProps extends Omit<JSX.IntrinsicElements["button"], "class"> {
  pressed: boolean;
  size?: Size;
}

export function Toggle({ pressed, size = "group", ...rest }: ToggleProps) {
  const tone = pressed ? ON[size] : OFF[size];
  return <button type="button" aria-pressed={pressed} class={`${SHAPE[size]} ${tone}`} {...rest} />;
}

/** The trough a row of `segment` toggles sits in. */
export const SEGMENT_TROUGH =
  "flex items-center gap-0.5 p-0.5 rounded-md border border-line bg-panel-2/70";

/** The trough a row of `touch-segment` toggles sits in, on the view. */
export const TOUCH_TROUGH =
  "flex items-center gap-0.5 p-1 rounded-full border border-line bg-glass/86 backdrop-blur-lg";

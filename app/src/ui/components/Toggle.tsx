/**
 * A button in a bar that is either the chosen one or not.
 *
 * `pressed` is the whole state, and it reaches the DOM as `aria-pressed` as
 * well as a colour, so a screen reader is told what the accent is telling a
 * sighted user. Shape and colour stay in separate strings for the reason
 * Button gives.
 *
 * Two sizes because the bar and the segmented control genuinely differ: the op
 * palette's groups carry an icon and a word and sit on their own, the view
 * tools' segments sit shoulder to shoulder in a trough.
 */

import type { JSX } from "preact";

type Size = "group" | "segment";

const SHAPE: Record<Size, string> = {
  group: "flex items-center gap-1.5 px-2 py-1 rounded-md border cursor-pointer",
  segment: "flex items-center gap-1 px-1.5 py-0.5 rounded cursor-pointer",
};

const OFF: Record<Size, string> = {
  group: "text-ink-dim border-transparent hover:text-ink hover:border-line",
  segment: "text-ink-dim hover:text-ink",
};

const ON: Record<Size, string> = {
  group: "text-accent bg-accent-deep/45 border-accent-edge",
  segment: "text-accent bg-accent-deep/70",
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

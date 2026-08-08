/** A single-line text input, and the uppercase micro-label that sits above one. */

import type { ComponentChildren, JSX } from "preact";
import { forwardRef } from "preact/compat";

const INPUT =
  "w-full px-2.5 py-[7px] bg-panel-2 text-ink border border-line rounded-md " +
  "focus:outline-none focus:border-accent";

export interface FieldProps extends Omit<JSX.IntrinsicElements["input"], "class" | "ref"> {
  /** Layout only — where this one sits. */
  layout?: string;
}

export const Field = forwardRef<HTMLInputElement, FieldProps>(({ layout, ...rest }, ref) => (
  <input ref={ref} class={`${INPUT} ${layout ?? ""}`} {...rest} />
));
Field.displayName = "Field";

export function Caption({ children }: { children: ComponentChildren }) {
  return <label class="block mb-1.5 text-ink-dim text-small">{children}</label>;
}

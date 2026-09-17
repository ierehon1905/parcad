/** A full-width strip under the titlebar: something to act on, never an error. */

import type { ComponentChildren, JSX } from "preact";

export interface BannerProps extends Omit<JSX.IntrinsicElements["div"], "class"> {
  /** Hides the strip; omitted, the strip has no close button. */
  onDismiss?: () => void;
  dismissLabel?: string;
  children?: ComponentChildren;
}

export function Banner({ onDismiss, dismissLabel = "Hide this bar", children, ...rest }: BannerProps) {
  return (
    <div
      class="flex flex-none flex-wrap items-center gap-x-3 gap-y-1 px-3 py-1.5
             bg-accent-deep/40 border-b border-accent-edge text-small text-ink"
      {...rest}
    >
      {children}
      {onDismiss && (
        <button
          type="button"
          aria-label={dismissLabel}
          class="text-ink-dim hover:text-ink cursor-pointer px-1"
          onClick={onDismiss}
        >
          ×
        </button>
      )}
    </div>
  );
}

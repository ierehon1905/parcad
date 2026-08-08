/**
 * Close this when the pointer goes down outside it, or Escape is pressed.
 *
 * On the capture phase, which is the part worth keeping: a press on another
 * group's button has to close the open one *before* that button's own handler
 * runs, or the two fight over the same signal and the menu flickers shut and
 * open again.
 *
 * Not what `PartMenu` needs — a context menu is opened by a click that would
 * otherwise be the click that closes it, so that one registers a one-shot
 * listener a frame later and deliberately stays separate.
 */

import { useEffect } from "preact/hooks";
import type { RefObject } from "preact";

export function useDismiss(
  inside: RefObject<HTMLElement>,
  close: () => void,
  options: {
    /** A second thing that counts as inside — an open flyout is not in the bar. */
    or?: (target: Node) => boolean;
    /** For anything positioned from a measured rectangle, which a resize invalidates. */
    onResize?: boolean;
  } = {},
) {
  const { or, onResize } = options;
  useEffect(() => {
    const down = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && !inside.current?.contains(target) && !or?.(target)) close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    document.addEventListener("pointerdown", down, true);
    document.addEventListener("keydown", escape);
    if (onResize) window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", down, true);
      document.removeEventListener("keydown", escape);
      if (onResize) window.removeEventListener("resize", close);
    };
  }, []);
}

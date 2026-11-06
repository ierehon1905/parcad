/**
 * Keep every touch that starts on `element` from reaching whatever scrolls
 * around the page. `touch-action: none` is only advice to a native scroll view
 * that holds the page; a cancelled touch is not. Pointer events still arrive.
 */
export function holdTouches(element: HTMLElement, only: (target: EventTarget | null) => boolean = () => true) {
  const cancel = (event: Event) => {
    if (only(event.target)) event.preventDefault();
  };
  for (const kind of ["touchstart", "touchmove"]) element.addEventListener(kind, cancel, { passive: false });
  return () => {
    for (const kind of ["touchstart", "touchmove"]) element.removeEventListener(kind, cancel);
  };
}

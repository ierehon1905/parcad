/**
 * The draggable split between the editor and the viewport.
 *
 * It writes a width straight onto the editor pane rather than into a signal.
 * That is deliberate: a pointer-move at 120 Hz through a signal would re-render
 * both panes on every frame of a drag, and the only thing that actually changes
 * is one CSS length. The DOM is the right place to keep a value nothing else
 * reads.
 */

import { useRef } from "preact/hooks";


export function Splitter() {
  // A ref rather than a local: a re-render would reset a plain variable
  // mid-drag, and the pointer would keep moving with nothing following it.
  const dragging = useRef(false);

  return (
    <div
      class="flex-none w-px bg-line cursor-col-resize hover:bg-accent"
      onPointerDown={(e) => {
        dragging.current = true;
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        if (!dragging.current) return;
        const pane = document.getElementById("editor-pane");
        if (!pane) return;
        const pct = (e.clientX / window.innerWidth) * 100;
        pane.style.width = `${Math.min(75, Math.max(15, pct))}%`;
      }}
      onPointerUp={(e) => {
        dragging.current = false;
        e.currentTarget.releasePointerCapture(e.pointerId);
      }}
    />
  );
}

/**
 * Selection during a drag.
 *
 * Text selection is *on* everywhere in this window, deliberately: a kernel
 * refusal, a measured number in the report, a selector in the inspector are all
 * things a person or an agent copies out, and a default of `select-none` with
 * panes opting back in fails the wrong way — the pane someone forgets is an
 * error message that can no longer be pasted. The only thing a global
 * suppression was ever for is the smear a *non-text* drag leaves across
 * whatever the pointer passes over, so that is the only thing suppressed, and
 * only while such a drag is running.
 *
 * Every drag surface in the app holds this for the length of its gesture: the
 * splitter, the number dial's scrub, the viewport's orbit. A surface that
 * forgets smears for that one drag and nothing else; nothing becomes
 * uncopyable. `app/src/selection.test.ts` keeps this the only place in the
 * frontend that touches `user-select` at all.
 */

/** Overlapping gestures: a stray `pointerup` must not unlock a live drag. */
let held = 0;

/** Suppress selection until the returned function is called. Idempotent. */
export function beginDrag(): () => void {
  held += 1;
  document.documentElement.dataset.dragging = "";
  let released = false;
  return () => {
    if (released) return;
    released = true;
    held -= 1;
    if (held === 0) delete document.documentElement.dataset.dragging;
  };
}

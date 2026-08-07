/**
 * The editor's decorations, defined where every half can reach them.
 *
 * A viewport hover marks the source call that generated the edge under the
 * pointer; a palette insertion flashes the call it just wrote. In both cases
 * the extension belongs to the editor and the *decision* belongs to something
 * else, so the effects and fields live here rather than in either — a module
 * they can all import without importing each other.
 */

import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";

/** Mark a source range without moving the cursor or the scroll position. */
export const setTreatmentHover = StateEffect.define<{ from: number; to: number } | undefined>();

export const treatmentHoverField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(decorations, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setTreatmentHover)) {
        return effect.value
          ? Decoration.set([
              Decoration.mark({ class: "cm-treatment-hover" }).range(effect.value.from, effect.value.to),
            ])
          : Decoration.none;
      }
    }
    return decorations.map(transaction.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});

/**
 * Mark a span as just-written, or clear the mark.
 *
 * The range is in the coordinates of the document *after* the transaction that
 * carries this effect, so an insertion can flash itself in the same dispatch
 * that performs it.
 */
export const flashInsert = StateEffect.define<{ from: number; to: number } | undefined>();

/**
 * The brief highlight over text the palette wrote.
 *
 * Pressing an operation writes at the caret, which is easy to be looking away
 * from — the button is at the top of the pane and the caret can be anywhere in
 * the file, or at line 1 if the editor never had focus. Without a flash the
 * honest reading of a press is "nothing happened", which is what it looked like
 * the first time anyone tried it.
 *
 * It maps through subsequent changes rather than being pinned to fixed offsets,
 * so typing over the selected argument — the very next thing anyone does —
 * keeps the highlight on the text rather than leaving it behind on stale
 * positions.
 */
export const insertFlashField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(decorations, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(flashInsert)) {
        return effect.value && effect.value.to > effect.value.from
          ? Decoration.set([
              Decoration.mark({ class: "cm-insert-flash" }).range(effect.value.from, effect.value.to),
            ])
          : Decoration.none;
      }
    }
    return decorations.map(transaction.changes);
  },
  provide: (field) => EditorView.decorations.from(field),
});

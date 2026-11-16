/**
 * The editor's half of "which line made this face": the line lit, and a face's
 * story as numbered steps in the gutter and on the scrollbar.
 *
 * Like `editor-marks.ts`, the effect and the field live here and the decision
 * in `made-by.ts`, so neither the editor nor the viewport imports the other.
 */

import { RangeSet, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, GutterMarker, ViewPlugin, gutter, type ViewUpdate } from "@codemirror/view";

export interface MadeByMarks {
  /** The line the viewport is pointing at, or that made what is shown. */
  lit?: number;
  /** A pinned face's story, a line per step in order; empty when none is pinned. */
  steps: number[];
  /** Which of `steps` is shown. */
  current: number;
}

const NONE: MadeByMarks = { steps: [], current: 0 };

export const setMadeBy = StateEffect.define<MadeByMarks>();

export const madeByField = StateField.define<MadeByMarks>({
  create: () => NONE,
  update(marks, transaction) {
    for (const effect of transaction.effects) if (effect.is(setMadeBy)) return effect.value;
    // Lines move under an edit, and these name lines of the last build.
    return transaction.docChanged ? NONE : marks;
  },
});

/** The lit line's background. */
const litLine = EditorView.decorations.compute([madeByField], (state) => {
  const { lit } = state.field(madeByField);
  if (lit === undefined || lit < 1 || lit > state.doc.lines) return Decoration.none;
  return Decoration.set([Decoration.line({ class: "cm-made-by-line" }).range(state.doc.line(lit).from)]);
});

class Step extends GutterMarker {
  constructor(
    readonly n: number,
    readonly on: boolean,
  ) {
    super();
  }
  eq(other: Step) {
    return other.n === this.n && other.on === this.on;
  }
  toDOM() {
    const badge = document.createElement("span");
    badge.textContent = String(this.n);
    badge.className = this.on
      ? "inline-grid place-items-center size-3.5 rounded-full bg-accent text-[9.5px] font-semibold font-sans leading-none text-bg"
      : "inline-grid place-items-center size-3.5 rounded-full border border-accent-edge text-[9.5px] font-semibold font-sans leading-none text-accent";
    return badge;
  }
}

/** A step's number beside its line, the one shown filled. */
const steps = gutter({
  class: "cm-made-by-steps",
  markers: (view) => {
    const { steps: lines, current } = view.state.field(madeByField);
    const doc = view.state.doc;
    const marks = lines
      .map((line, i) => ({ line, i }))
      .filter(({ line }) => line >= 1 && line <= doc.lines)
      .sort((a, b) => a.line - b.line)
      .map(({ line, i }) => new Step(i + 1, i === current).range(doc.line(line).from));
    return RangeSet.of(marks, true);
  },
  initialSpacer: () => new Step(8, false),
});

/** The steps on the scrollbar too, so a step scrolled out of sight is not one that is missing. */
const ruler = ViewPlugin.fromClass(
  class {
    readonly dom: HTMLElement;
    constructor(readonly view: EditorView) {
      this.dom = document.createElement("div");
      this.dom.className = "pointer-events-none absolute right-0 top-0 bottom-0 w-2 z-10";
      view.dom.appendChild(this.dom);
      this.draw();
    }
    update(update: ViewUpdate) {
      if (update.geometryChanged || update.startState.field(madeByField) !== update.state.field(madeByField)) this.draw();
    }
    draw() {
      const { steps: lines, current } = this.view.state.field(madeByField);
      const doc = this.view.state.doc;
      const height = this.view.contentHeight || 1;
      this.dom.replaceChildren(
        ...lines
          .map((line, i) => ({ line, i }))
          .filter(({ line }) => line >= 1 && line <= doc.lines)
          .map(({ line, i }) => {
            const tick = document.createElement("span");
            tick.className = i === current ? "absolute inset-x-0.5 h-1 -mt-0.5 rounded-xs bg-accent" : "absolute inset-x-0.5 h-1 -mt-0.5 rounded-xs bg-accent-edge";
            const block = this.view.lineBlockAt(doc.line(line).from);
            tick.style.top = `${((block.top + block.height / 2) / height) * 100}%`;
            return tick;
          }),
      );
    }
    destroy() {
      this.dom.remove();
    }
  },
);

/**
 * The accent, as the faces on the part are lit. A theme rather than
 * `style.css`: the active line is One Dark's, and an unlayered theme rule
 * outranks anything in a layer whatever its specificity.
 */
const theme = EditorView.theme({
  ".cm-line.cm-made-by-line, .cm-line.cm-made-by-line.cm-activeLine": {
    backgroundColor: "color-mix(in srgb, var(--color-accent) 18%, transparent)",
  },
  ".cm-made-by-steps .cm-gutterElement": { padding: "0 2px 0 4px", display: "flex", alignItems: "center" },
});

export const madeByMarks: Extension = [madeByField, litLine, steps, ruler, theme];

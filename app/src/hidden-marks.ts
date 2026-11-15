/**
 * Bodies hidden from the view, marked where the script writes them: the
 * statement greyed like a comment, and a chip after the name that shows the
 * body again.
 *
 * A mark and never an edit. Hiding is looking: written into `part.js` it would
 * turn a saved part unsaved, land in its history, and race an agent writing
 * the same file. Commenting the body out is deleting it — it would leave the
 * measurements, the export and the problems with it.
 */

import { syntaxTree } from "@codemirror/language";
import { StateEffect, StateField, type EditorState, type Range } from "@codemirror/state";
import { Decoration, EditorView, WidgetType, type DecorationSet } from "@codemirror/view";

interface Hidden {
  names: readonly string[];
  show: (name: string) => void;
  marks: DecorationSet;
}

/** Which bodies are hidden, and what pressing a chip does about it. */
export const setHiddenBodies = StateEffect.define<{ names: readonly string[]; show: (name: string) => void }>();

export const hiddenBodiesField = StateField.define<Hidden>({
  create: () => ({ names: [], show: () => {}, marks: Decoration.none }),
  update(value, transaction) {
    let next = value;
    for (const effect of transaction.effects) {
      if (effect.is(setHiddenBodies)) next = { ...effect.value, marks: value.marks };
    }
    if (next === value && !transaction.docChanged) return value;
    return { ...next, marks: marksFor(transaction.state, next.names, next.show) };
  },
  provide: (field) => EditorView.decorations.from(field, (value) => value.marks),
});

const escaped = (name: string) => name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Where the script names a body: its declaration, or failing that its key in the returned object. */
export function bodyNameAt(source: string, name: string): number | undefined {
  for (const pattern of [
    `\\b(?:const|let|var)\\s+${escaped(name)}\\b`,
    `\\breturn\\s*\\{[^}]*\\b${escaped(name)}\\b`,
  ]) {
    const match = new RegExp(pattern).exec(source);
    if (match) return match.index + match[0].length - name.length;
  }
  return undefined;
}

function marksFor(state: EditorState, names: readonly string[], show: (name: string) => void): DecorationSet {
  if (!names.length) return Decoration.none;
  const source = state.doc.toString();
  const tree = syntaxTree(state);
  const marks: Range<Decoration>[] = [];
  for (const name of names) {
    const at = bodyNameAt(source, name);
    if (at === undefined) continue;
    // The whole statement that writes it, or the one property of the returned object.
    let node = tree.resolveInner(at, 1);
    while (node.parent && node.name !== "VariableDeclaration" && node.name !== "Property") node = node.parent;
    const whole = node.name === "VariableDeclaration" || node.name === "Property";
    const from = whole ? node.from : at;
    const to = whole ? node.to : at + name.length;
    marks.push(Decoration.mark({ class: "cm-hidden-body" }).range(from, to));
    // Right after the name, which is near the left edge however long the line runs.
    marks.push(Decoration.widget({ widget: new ShowChip(name, show), side: 1 }).range(at + name.length));
  }
  return Decoration.set(marks, true);
}

class ShowChip extends WidgetType {
  constructor(
    readonly name: string,
    readonly show: (name: string) => void,
  ) {
    super();
  }

  eq(other: ShowChip) {
    return other.name === this.name;
  }

  toDOM() {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "cm-hidden-chip";
    chip.textContent = "hidden";
    chip.title = `Show ${this.name} again`;
    // Before the editor takes the press as a click that moves the caret.
    chip.addEventListener("mousedown", (e) => {
      e.preventDefault();
      this.show(this.name);
    });
    return chip;
  }

  ignoreEvent() {
    return true;
  }
}

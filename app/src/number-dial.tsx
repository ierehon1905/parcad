/**
 * Click a number, then dial it with the arrow keys.
 *
 * Playing with a part means changing one number and watching the geometry move,
 * and until now that cost a text edit per try: select three characters, retype
 * them, don't fumble the decimal point. The evaluation loop is already 120 ms —
 * the slow half was the typing.
 *
 * A click on a numeric literal *engages* it: the number is marked, a pair of
 * chevrons appears beside it, and Up/Down step the value. Engaging on the
 * pointer and not on the caret is the whole trick. If the caret landing on a
 * number were enough, Up and Down would stop moving between lines every time
 * keyboard navigation crossed one, which is a worse loss than this is a gain.
 * Clicking is unambiguous: you went to that number on purpose.
 *
 * Steps follow the CSS pane in the browser's own developer tools, because that
 * is the dial most people have already learnt: 1, or 0.1 when the value is
 * already a fraction, ⇧ for 10, ⌥ for 0.1. Dragging the chevrons scrubs
 * continuously.
 *
 * Only literals the syntax tree calls numbers are dialled, which is what keeps
 * it away from the digits inside a selector string — `">Z and |X"` is text, and
 * `edge@3` is text, and neither should move when something nudges an arrow key.
 */

import { syntaxTree } from "@codemirror/language";
import { Prec, StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { isolateHistory } from "@codemirror/commands";
import { Decoration, EditorView, WidgetType, keymap } from "@codemirror/view";
import { render } from "preact";

import { tip } from "./ui/tooltip";

/** The literal being dialled, as a document range. */
interface Dial {
  from: number;
  to: number;
}

const setDial = StateEffect.define<Dial | undefined>();

/**
 * Which literal is engaged.
 *
 * It maps through the document rather than being recomputed, so a step that
 * rewrites `9` as `10` keeps the dial on the number it just grew.
 */
const dialField = StateField.define<Dial | undefined>({
  create: () => undefined,
  update(dial, transaction) {
    for (const effect of transaction.effects) if (effect.is(setDial)) return effect.value;
    if (!dial) return undefined;
    if (!transaction.docChanged) return dial;
    const from = transaction.changes.mapPos(dial.from, 1);
    const to = transaction.changes.mapPos(dial.to, -1);
    return to > from ? { from, to } : undefined;
  },
  provide: (field) =>
    EditorView.decorations.from(field, (dial) =>
      dial
        ? Decoration.set([
            Decoration.mark({ class: "cm-number-dial" }).range(dial.from, dial.to),
            Decoration.widget({ widget: CHEVRONS, side: 1 }).range(dial.to),
          ])
        : Decoration.none,
    ),
});

/** The literal under an offset, counting both edges, with a unary minus if it has one. */
export function numberAt(state: EditorState, pos: number): Dial | undefined {
  const tree = syntaxTree(state);
  let node = tree.resolveInner(pos, -1);
  if (node.name !== "Number") node = tree.resolveInner(pos, 1);
  if (node.name !== "Number") return undefined;

  // `-5` is an operator applied to `5`. Dialling the digits alone would step
  // the magnitude — up from -5 would reach -6 — and could never cross zero.
  const parent = node.parent;
  const negated =
    parent?.name === "UnaryExpression" &&
    parent.to === node.to &&
    state.doc.sliceString(parent.from, node.from).trim() === "-";
  return { from: negated ? parent.from : node.from, to: node.to };
}

/**
 * The literal stepped by `step`, or nothing if it is not a plain decimal.
 *
 * Hex, exponents and separators are all legal JavaScript and all lose their
 * notation the moment arithmetic touches them, so they are left alone rather
 * than silently rewritten as something else that happens to be equal.
 *
 * The result keeps as many decimal places as the larger of the literal and the
 * step, which is both what stops 1.2 + 0.1 from arriving as 1.3000000000000003
 * and what lets a number written `2.50` stay at that precision.
 */
export function stepped(text: string, step: number): string | undefined {
  if (!/^-?(\d+\.?\d*|\.\d+)$/.test(text)) return undefined;
  const value = Number(text);
  if (!Number.isFinite(value)) return undefined;
  return (value + step).toFixed(Math.max(decimals(text), decimals(String(step))));
}

function decimals(text: string): number {
  const point = text.indexOf(".");
  return point < 0 ? 0 : text.length - point - 1;
}

/**
 * How far one press moves.
 *
 * The plain step is 1 mm, which is the unit nearly every number in a part is
 * written in. A value already smaller than 1 is not a length in that sense —
 * it is a tolerance, a scale, a fraction — and stepping it by 1 would throw it
 * away, so it steps by 0.1 instead.
 */
export function stepFor(text: string, event: { shiftKey: boolean; altKey: boolean }): number {
  if (event.shiftKey && event.altKey) return 0.01;
  if (event.shiftKey) return 10;
  if (event.altKey) return 0.1;
  return Math.abs(Number(text)) < 1 ? 0.1 : 1;
}

/**
 * Apply one step.
 *
 * The transaction carries no `userEvent`, which is what lets the editor's own
 * history join a run of steps into a single undo entry: anything within half a
 * second of the last change, touching the same span, is the same gesture. One
 * press is one undo; a drag is one undo; a step after a pause is its own.
 */
function step(view: EditorView, by: number): boolean {
  const dial = view.state.field(dialField, false);
  if (!dial) return false;
  const text = view.state.doc.sliceString(dial.from, dial.to);
  const next = stepped(text, by);
  if (next === undefined) return false;
  view.dispatch({
    changes: { from: dial.from, to: dial.to, insert: next },
    scrollIntoView: true,
  });
  return true;
}

/** Engage a literal, isolating what follows from whatever was typed before it. */
function engage(view: EditorView, dial: Dial | undefined) {
  const current = view.state.field(dialField, false);
  if (current?.from === dial?.from && current?.to === dial?.to) return;
  view.dispatch({
    effects: setDial.of(dial),
    annotations: dial ? isolateHistory.of("before") : [],
  });
}

/** How far the pointer travels for one step while scrubbing. */
const SCRUB_PX = 4;

/** The literal currently engaged, as text. */
function dialText(view: EditorView): string | undefined {
  const dial = view.state.field(dialField, false);
  return dial && view.state.doc.sliceString(dial.from, dial.to);
}

/**
 * The chevrons.
 *
 * An inline widget with a real width, the way an editor draws an inlay hint,
 * rather than something floating over the line. Floating would have been free
 * of reflow and would have covered the character after the number, and in
 * `box(10, 20)` that character is the comma.
 */
function Chevrons({ view }: { view: EditorView }) {
  const press = (direction: 1 | -1) => (event: PointerEvent) => {
    event.preventDefault();
    const origin = event.clientY;
    let applied = 0;

    const move = (e: PointerEvent) => {
      const want = Math.round((origin - e.clientY) / SCRUB_PX);
      if (want === applied) return;
      const text = dialText(view);
      if (text !== undefined) step(view, (want - applied) * stepFor(text, e));
      applied = want;
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      // A press that never moved is an ordinary click on one of the two
      // chevrons. The listeners are on the window rather than on this element
      // because a capture taken on an element the editor may replace during
      // the drag would be dropped halfway down.
      const text = applied === 0 ? dialText(view) : undefined;
      if (text !== undefined) step(view, direction * stepFor(text, event));
      view.focus();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const chevron =
    "flex h-[0.62em] w-[1em] items-center justify-center text-ink-dim hover:text-accent";

  return (
    <span
      class="mx-0.5 inline-flex cursor-ns-resize select-none flex-col items-center rounded-xs bg-panel-2 align-middle ring-1 ring-line"
      {...tip({
        title: "dial this number",
        key: "↑ ↓",
        text: "⇧ steps by 10, ⌥ by 0.1. Drag to scrub.",
      })}
    >
      <button type="button" class={chevron} onPointerDown={press(1)} aria-label="increase">
        <Chevron up />
      </button>
      <button type="button" class={chevron} onPointerDown={press(-1)} aria-label="decrease">
        <Chevron />
      </button>
    </span>
  );
}

function Chevron({ up }: { up?: boolean }) {
  return (
    <svg viewBox="0 0 8 5" class="h-[0.4em] w-[0.62em]" aria-hidden="true">
      <path
        d={up ? "M1 4 L4 1 L7 4" : "M1 1 L4 4 L7 1"}
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  );
}

/**
 * One widget for every literal, on purpose.
 *
 * `eq` is unconditionally true so that stepping — which moves the decoration,
 * because the text under it just changed length — reuses the element instead of
 * replacing it. A replaced element would drop the pointer mid-drag, and the
 * drag is the part that has to stay smooth.
 */
class ChevronWidget extends WidgetType {
  eq() {
    return true;
  }

  toDOM(view: EditorView): HTMLElement {
    const dom = document.createElement("span");
    dom.className = "cm-number-dial-widget";
    render(<Chevrons view={view} />, dom);
    return dom;
  }

  destroy(dom: HTMLElement) {
    render(null, dom);
  }

  ignoreEvent() {
    return true;
  }
}

const CHEVRONS = new ChevronWidget();

const inWidget = (target: EventTarget | null) =>
  target instanceof Element && !!target.closest(".cm-number-dial-widget");

function dialStep(view: EditorView, shiftKey: boolean, altKey: boolean): number {
  const text = dialText(view);
  return text === undefined ? 0 : stepFor(text, { shiftKey, altKey });
}

/** Both directions of every modifier combination, as editor key bindings. */
const DIAL_KEYS = [
  ["", false, false],
  ["Shift-", true, false],
  ["Alt-", false, true],
  ["Shift-Alt-", true, true],
] as const;

/** Dialling, as an editor extension. */
export const numberDial: Extension = [
  dialField,
  // Above the default keymap, so an engaged dial gets the arrow keys before
  // the cursor does. Nothing else is taken: every other key falls through to
  // the handler below, which lets go of the dial on the way past.
  Prec.high([
    keymap.of(
      DIAL_KEYS.flatMap(([prefix, shift, alt]) => [
        { key: `${prefix}ArrowUp`, run: (view: EditorView) => step(view, dialStep(view, shift, alt)) },
        { key: `${prefix}ArrowDown`, run: (view: EditorView) => step(view, -dialStep(view, shift, alt)) },
      ]),
    ),
    EditorView.domEventHandlers({
      mouseup(event, view) {
        if (inWidget(event.target)) return false;
        const { main } = view.state.selection;
        engage(view, main.empty ? numberAt(view.state, main.head) : undefined);
        return false;
      },
      keydown(event, view) {
        if (!view.state.field(dialField, false)) return false;
        // Up and Down have already had their chance above; a modifier on its
        // own is someone reaching for one of them. Everything else — typing,
        // Escape, undo — ends the engagement, and undo in particular has to
        // see the history entry closed before it runs.
        if (event.key === "ArrowUp" || event.key === "ArrowDown") return false;
        if (["Shift", "Alt", "Meta", "Control"].includes(event.key)) return false;
        engage(view, undefined);
        return false;
      },
      blur(_event, view) {
        engage(view, undefined);
        return false;
      },
    }),
  ]),
];

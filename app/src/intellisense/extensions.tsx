/**
 * The language service, wired to the editor.
 *
 * Four surfaces, all reading the one analyzer: completion with the real
 * signature beside each entry, a hover card, the parameter you are currently
 * typing, and diagnostics. Each is asynchronous because the service is on
 * another thread, and each is allowed to answer "nothing" — a completion list
 * that has not arrived must never be the reason a keystroke is slow.
 */

import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { type Diagnostic, linter } from "@codemirror/lint";
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, showTooltip, type Tooltip } from "@codemirror/view";
import { render } from "preact";

import type { SignatureInfo } from "./analyzer";
import { Card, Code, InfoCard } from "./card";
import * as service from "./service";

/**
 * Render a Preact card into the element CodeMirror positions for us.
 *
 * Background, border, radius, shadow and the scrolling are on `.cm-tooltip` in
 * `style.css`, because CodeMirror renders that element and there is nowhere to
 * put a class on it. Height is its business too — it shrinks a card to the room
 * `tooltipSpace` says there is. What is left here is how wide one may grow.
 *
 * How wide one may grow is `Card`'s, which is the box each of these draws.
 */
function card(draw: (into: HTMLElement) => void, name?: string) {
  const dom = document.createElement("div");
  // CodeMirror adds `cm-tooltip` to a tooltip's own element and names the hover
  // and the completion list itself. The parameter hints are the one floating
  // box with nothing to style them by, so they are named here — and the
  // completion details panel is deliberately *not*, because the element this
  // builds for it goes inside a panel CodeMirror has already sized.
  if (name) dom.className = name;
  draw(dom);
  return { dom, destroy: () => render(null, dom) };
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

/** TypeScript's kinds, as CodeMirror names them for the icon beside an entry. */
const COMPLETION_TYPE: Record<string, string> = {
  function: "function",
  method: "method",
  property: "property",
  const: "constant",
  let: "variable",
  var: "variable",
  parameter: "variable",
  class: "class",
  interface: "interface",
  type: "type",
  enum: "enum",
  keyword: "keyword",
  module: "namespace",
};

async function complete(context: CompletionContext): Promise<CompletionResult | null> {
  const word = context.matchBefore(/[\w$]*/);
  const member = context.matchBefore(/\.\s*[\w$]*$/);
  if (!member && !context.explicit && (!word || word.from === word.to)) return null;

  const entries = await service.completions(context.pos);
  if (!entries?.length) return null;

  return {
    from: word ? word.from : context.pos,
    options: entries.map((entry): Completion => ({
      label: entry.label,
      type: COMPLETION_TYPE[entry.kind] ?? entry.kind,
      apply: entry.insert,
      // TypeScript has already ranked these — a local above a global, a member
      // of the right type above one of any type — and that ranking is worth
      // more than alphabetical. Its sort keys start at "11".
      boost: 99 - Math.min(99, Number(entry.sortText.slice(0, 2)) || 50),
      info: () =>
        service.completionDetail(context.pos, entry.label).then((info) =>
          info
            ? card((into) =>
                render(
                  <Card>
                    <InfoCard info={info} />
                  </Card>,
                  into,
                ),
              )
            : null,
        ),
    })),
    validFor: /^[\w$]*$/,
  };
}

// ---------------------------------------------------------------------------
// Signature help
// ---------------------------------------------------------------------------

const setSignature = StateEffect.define<Tooltip | null>();

/**
 * The call you are inside, above the caret.
 *
 * A `StateField` rather than a hover: the question is not "what is under the
 * pointer" but "where is the caret", and the answer has to survive every
 * keystroke that types an argument.
 */
const signatureField = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, transaction) {
    for (const effect of transaction.effects) if (effect.is(setSignature)) return effect.value;
    // A tooltip pinned to a position the edit moved is a tooltip pointing at
    // the wrong argument, so it goes as soon as the document does.
    if (transaction.docChanged || transaction.selection) return null;
    return value;
  },
  provide: (field) => showTooltip.from(field),
});

function SignatureBar({ help }: { help: SignatureInfo }) {
  const item = help.signatures[help.selected] ?? help.signatures[0];
  if (!item) return null;
  const active = item.params[help.argument];
  return (
    <div class="font-mono text-small">
      <div>
        <Code spans={item.prefix} />
        {item.params.map((param, i) => (
          <>
            {i > 0 && <span class="text-ink-dim">, </span>}
            <Code spans={param.label} class={i === help.argument ? "" : "opacity-45"} />
          </>
        ))}
        <Code spans={item.suffix} />
      </div>
      {active && active.doc.length > 0 && (
        <p class="m-0 mt-1 font-sans text-ink-dim">{active.doc.map((s) => s.text).join("")}</p>
      )}
      {help.signatures.length > 1 && (
        <p class="m-0 mt-1 font-sans text-ink-dim text-tiny">
          {help.selected + 1} of {help.signatures.length} overloads
        </p>
      )}
    </div>
  );
}

/** Ask for the call under the caret, and show or hide the bar accordingly. */
async function updateSignature(view: EditorView) {
  const pos = view.state.selection.main.head;
  const help = await service.signatureHelp(pos);
  // The caret has moved on while the worker was answering; a bar for where it
  // used to be is worse than none.
  if (view.state.selection.main.head !== pos) return;
  view.dispatch({
    effects: setSignature.of(
      help && help.signatures.length > 0
        ? {
            pos,
            above: true,
            create: () =>
              card(
                (into) =>
                  render(
                    <Card>
                      <SignatureBar help={help} />
                    </Card>,
                    into,
                  ),
                "cm-tooltip-signature",
              ),
          }
        : null,
    ),
  });
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

async function typeDiagnostics(view: EditorView): Promise<Diagnostic[]> {
  const problems = await service.diagnostics();
  const end = view.state.doc.length;
  return (problems ?? [])
    // A stale answer can outlive the text it was about; clamping keeps a mark
    // inside the document rather than throwing on dispatch.
    .filter((problem) => problem.from <= end)
    .map((problem) => ({
      from: problem.from,
      to: Math.min(problem.to, end),
      severity: problem.severity,
      message: problem.message,
      source: "typescript",
    }));
}

// ---------------------------------------------------------------------------

/** Everything above, as one extension. */
export function intellisense(): Extension {
  return [
    autocompletion({ override: [complete], icons: true, activateOnTyping: true }),
    signatureField,
    linter(typeDiagnostics, { delay: 400 }),
    EditorView.updateListener.of((update) => {
      if (update.docChanged) service.setPart(update.state.doc.toString());
      if (update.docChanged || update.selectionSet) void updateSignature(update.view);
    }),
  ];
}

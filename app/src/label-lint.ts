/**
 * Underline a `checks:` written as a statement of its own, where it was typed.
 *
 * JavaScript reads it as a label, so the build refuses it with the line and
 * the fix; this puts the same refusal on the word itself, as the selector
 * linter does for a bad term. The DSL finds it by asking the engine, so the
 * editor and every runner agree on what is a label.
 */

import { type Diagnostic, linter } from "@codemirror/lint";
import type { EditorState } from "@codemirror/state";

import { __parcadLabelledPartKeys } from "./dsl";

/** One diagnostic per `checks:` label in the document. */
export function labelDiagnostics(state: EditorState): Diagnostic[] {
  return __parcadLabelledPartKeys(state.doc.toString()).map(({ from, to, message }) => ({
    from,
    to,
    severity: "error",
    source: "parcad",
    message,
  }));
}

/** The CodeMirror extension. */
export const labelLinter = linter((view) => labelDiagnostics(view.state));

/**
 * Underline a bad selector term where it was typed.
 *
 * Without this, `shape.edges(">Q")` is only a build error: the message names
 * the fault but not the place, and the viewport keeps showing the last good
 * geometry while the offending character sits unmarked in the source. The
 * parser already returns a span per term, so the editor can point at the two
 * characters actually responsible.
 *
 * This lints syntax only. Whether a valid selector resolves to any edges — or
 * to the count an `expect()` demands — is a question about evaluated geometry,
 * which only the kernel can answer, and it is reported from there.
 */

import { syntaxTree } from "@codemirror/language";
import { type Diagnostic, linter } from "@codemirror/lint";
import type { EditorState } from "@codemirror/state";

import { parseEdgeSelector, parseVertexSelector, SelectorSyntaxError } from "./selectors";

const SELECTOR_METHODS: Record<string, (source: string) => unknown> = {
  edges: parseEdgeSelector,
  vertices: parseVertexSelector,
};

/** Syntax diagnostics for every literal selector string in the document. */
export function selectorDiagnostics(state: EditorState): Diagnostic[] {
  const source = state.doc.toString();
  const diagnostics: Diagnostic[] = [];

  syntaxTree(state).iterate({
    enter(node) {
      if (node.name !== "PropertyName") return;
      const parse = SELECTOR_METHODS[source.slice(node.from, node.to)];
      if (!parse) return;

      // `.edges` as a property rather than a call — `{ edges: 2 }` — selects
      // nothing and has no string to check.
      const call = node.node.parent?.parent;
      if (call?.name !== "CallExpression") return;
      const args = call.getChild("ArgList");
      const literal = args?.firstChild?.nextSibling;
      // A computed selector is legal; it is simply not checkable until it runs,
      // and the DSL's own assertion still catches it then.
      if (literal?.name !== "String") return;

      const quoted = source.slice(literal.from, literal.to);
      const text = quoted.slice(1, -1);
      // An escape would shift every offset the parser reports. Rather than
      // guess at the mapping, leave it to the runtime assertion.
      if (text.includes("\\")) return;

      try {
        parse(text);
      } catch (e) {
        if (!(e instanceof SelectorSyntaxError)) throw e;
        // +1 for the opening quote. An empty span — an empty selector, or an
        // empty term after a stray separator — is widened to a caret the
        // editor can actually draw.
        const from = literal.from + 1 + e.from;
        const to = literal.from + 1 + e.to;
        diagnostics.push({
          from,
          to: to > from ? to : Math.min(from + 1, literal.to),
          severity: "error",
          source: "parcad selector",
          message: e.message,
        });
      }
    },
  });

  return diagnostics;
}

/** The CodeMirror extension. */
export const selectorLinter = linter((view) => selectorDiagnostics(view.state));

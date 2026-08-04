import { syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

export interface SourceLocation {
  line: number;
  column: number;
  method: string;
}

export interface SourceRange {
  from: number;
  to: number;
}

const TREATMENT_METHODS = new Set(["fillet", "chamfer", "smooth", "squircle"]);

export function sourceOffset(source: string, line: number, column: number): number | undefined {
  let at = 0;
  for (let current = 1; current < line; current++) {
    const next = source.indexOf("\n", at);
    if (next < 0) return undefined;
    at = next + 1;
  }
  return at + column - 1 <= source.length ? at + column - 1 : undefined;
}

/** The complete source call that contains a treatment method. */
export function treatmentCallRange(
  state: EditorState,
  source: string,
  location: SourceLocation,
): SourceRange | undefined {
  const start = sourceOffset(source, location.line, location.column);
  if (start === undefined) return undefined;

  // Chained calls nest CallExpression nodes. The closest one containing the
  // method includes `.edges(…)` and `.expect(…)`, while stopping before a
  // later `.tag(…)` on the treatment result.
  let node = syntaxTree(state).resolveInner(start, 1);
  while (true) {
    if (node.name === "CallExpression" && node.from <= start && node.to >= start) {
      return { from: node.from, to: node.to };
    }
    const parent = node.parent;
    if (!parent) break;
    node = parent;
  }

  // A script can be temporarily incomplete while typing. Keep the method mark
  // useful rather than retaining a stale highlight.
  return { from: start, to: start + location.method.length };
}

/**
 * Carry editor source locations into `new Function` explicitly.
 *
 * WebKit, V8, and SpiderMonkey format `Error.stack` differently. Wrapping the
 * evaluated call means every engine gives the same treatment location without
 * changing the user's source or the graph it produces.
 */
export function instrumentTreatmentCalls(state: EditorState, source: string): string {
  const prefixes = new Map<number, Array<{ to: number; text: string }>>();
  const suffixes = new Map<number, string[]>();

  syntaxTree(state).iterate({
    enter(node) {
      if (node.name !== "PropertyName") return;
      const method = source.slice(node.from, node.to);
      if (!TREATMENT_METHODS.has(method)) return;

      const location = locationAt(source, node.from, method);
      const range = treatmentCallRange(state, source, location);
      // A property such as `{ fillet: 1 }` is not a method call. The text
      // between its name and the enclosing call's end identifies the method
      // invocation without relying on a particular syntax-tree child layout.
      if (!range || !new RegExp(`^${method}\\s*\\(`).test(source.slice(node.from, range.to))) return;

      const locationJson = JSON.stringify(location);
      const entries = prefixes.get(range.from) ?? [];
      entries.push({
        to: range.to,
        text: `__parcadTreatmentSource(${locationJson}, () => `,
      });
      prefixes.set(range.from, entries);
      suffixes.set(range.to, [...(suffixes.get(range.to) ?? []), ")"]);
    },
  });

  const inserts = [
    ...[...prefixes].map(([at, entries]) => ({
      at,
      // Outer chained calls must wrap inner ones at the same start offset.
      text: entries.sort((a, b) => b.to - a.to).map((entry) => entry.text).join(""),
    })),
    ...[...suffixes].map(([at, entries]) => ({ at, text: entries.join("") })),
  ].sort((a, b) => b.at - a.at);

  let instrumented = source;
  for (const insert of inserts) {
    instrumented = instrumented.slice(0, insert.at) + insert.text + instrumented.slice(insert.at);
  }
  return instrumented;
}

function locationAt(source: string, offset: number, method: string): SourceLocation {
  const line = source.slice(0, offset).split("\n").length;
  const previousBreak = source.lastIndexOf("\n", offset - 1);
  return { line, column: offset - previousBreak, method };
}

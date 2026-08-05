/**
 * The compact edge- and vertex-selector grammar, parsed in the editor.
 *
 * This is a deliberate second implementation of
 * `crates/parcad-core/src/selectors.rs`. The kernel must keep its own parser —
 * a graph can arrive from anywhere, so the editor is never the gate — but the
 * editor needs the answer synchronously, on every keystroke, to underline a bad
 * term while it is being typed. An IPC round trip cannot do that, and a WASM
 * build of the core for forty lines of grammar costs more than it returns.
 *
 * What makes two implementations safe is that neither one is the specification:
 * `eval/selectors.json` is, and `selectors.test.ts` and the Rust
 * `agrees_with_the_shared_selector_corpus` test both run against it. Changing a
 * rule in one language and not the other fails a test rather than shipping an
 * editor that accepts what the kernel later refuses.
 *
 * Offsets here are UTF-16 code units and the Rust spans are bytes. Every term
 * in this grammar is ASCII, so the two agree; a non-ASCII selector is a syntax
 * error before the difference could matter.
 */

export type Axis = "X" | "Y" | "Z";

export type TermKind = "max" | "min" | "parallel";

/** One test in a selector: `>Z` is max Z, `<Y` min Y, `|X` parallel to X. */
export interface EdgeSelectorTerm {
  kind: TermKind;
  axis: Axis;
}

/** A parsed term and the range of the authored string it came from. */
export interface SpannedTerm extends EdgeSelectorTerm {
  /** Offset of the term within the selector string, not within the document. */
  from: number;
  to: number;
}

/**
 * A rejected selector, carrying the range of the text to blame.
 *
 * `from`/`to` are relative to the selector string, so an editor adds the
 * position of the opening quote to get a document range.
 */
export class SelectorSyntaxError extends Error {
  constructor(
    message: string,
    readonly from: number,
    readonly to: number,
  ) {
    super(message);
    this.name = "SelectorSyntaxError";
  }
}

/** The canonical source form of a term, as recorded in the shared corpus. */
export function termSource(term: EdgeSelectorTerm): string {
  return `${{ max: ">", min: "<", parallel: "|" }[term.kind]}${term.axis}`;
}

/** Parse a compact edge selector, or throw a {@link SelectorSyntaxError}. */
export function parseEdgeSelector(source: string): SpannedTerm[] {
  // Offsets stay relative to the untrimmed input: the caller knows where the
  // string literal starts, not where the parser decided the content did.
  const leading = source.length - source.trimStart().length;
  const trimmed = source.trim();
  if (!trimmed) {
    throw new SelectorSyntaxError(
      "edge selector is empty; use a term such as >Z or |X",
      0,
      source.length,
    );
  }

  // The separator is exactly `" and "`. Scanning for it rather than splitting
  // keeps each term's offset, and finds the same non-overlapping separators a
  // split would, so an oddly spaced selector is rejected the same way.
  const terms: SpannedTerm[] = [];
  for (let at = 0; ; ) {
    const found = trimmed.indexOf(" and ", at);
    const end = found < 0 ? trimmed.length : found;
    terms.push(parseTerm(trimmed.slice(at, end), leading + at, leading + end));
    if (found < 0) return terms;
    at = found + " and ".length;
  }
}

/**
 * Parse a compact vertex selector, or throw a {@link SelectorSyntaxError}.
 *
 * A vertex has a position but no direction, so `|X` is rejected here instead of
 * quietly meaning something different from its edge-selector counterpart.
 */
export function parseVertexSelector(source: string): SpannedTerm[] {
  const terms = parseEdgeSelector(source);
  const parallel = terms.find((term) => term.kind === "parallel");
  if (parallel) {
    throw new SelectorSyntaxError(
      "vertex selectors use only >X or <X extrema; |X applies to edges",
      parallel.from,
      parallel.to,
    );
  }
  return terms;
}

const AXES: Record<string, Axis> = { X: "X", Y: "Y", Z: "Z" };
const KINDS: Record<string, TermKind> = { ">": "max", "<": "min", "|": "parallel" };

function parseTerm(term: string, from: number, to: number): SpannedTerm {
  // Rust's `{:?}` and `JSON.stringify` quote and escape ASCII identically,
  // which is what keeps the two implementations' messages the same string.
  const quoted = JSON.stringify(term);
  if (term.length !== 2) {
    throw new SelectorSyntaxError(
      `invalid edge-selector term ${quoted}; expected >X, <Y, or |Z (joined with \`and\`)`,
      from,
      to,
    );
  }

  const axis = AXES[term[1].toUpperCase()];
  if (!axis) {
    throw new SelectorSyntaxError(
      `invalid edge-selector axis in ${quoted}; expected X, Y, or Z`,
      from,
      to,
    );
  }

  const kind = KINDS[term[0]];
  if (!kind) {
    throw new SelectorSyntaxError(
      `invalid edge-selector term ${quoted}; expected >X, <Y, or |Z`,
      from,
      to,
    );
  }

  return { kind, axis, from, to };
}

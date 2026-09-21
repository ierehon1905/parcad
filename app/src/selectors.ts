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

/**
 * The sentence a compact-form refusal adds when the author reached for a word
 * the compact form does not have. `wider_language` in `selectors.rs` says it
 * in the same words; `eval/selectors.json` holds the two together.
 */
export function widerLanguage(term: string): string {
  const lower = term.toLowerCase();
  const words = lower.split(/[^a-z]+/);
  const reached = ["not", "or"].some((word) => words.includes(word)) || term.includes("(") || term.includes(")");
  if (!reached) return "";
  return (
    ". The compact form has no not, or or brackets: say it in the query form instead, " +
    "which has dihedral, parallel, longerThan, on, between and not — " +
    '{ dihedral: "convex", not: { parallel: "z" } }. check_selector parses one without building anything'
  );
}

const AXES: Record<string, Axis> = { X: "X", Y: "Y", Z: "Z" };
const KINDS: Record<string, TermKind> = { ">": "max", "<": "min", "|": "parallel" };

function parseTerm(term: string, from: number, to: number): SpannedTerm {
  // Rust's `{:?}` and `JSON.stringify` quote and escape ASCII identically,
  // which is what keeps the two implementations' messages the same string.
  const quoted = JSON.stringify(term);
  if (term.length !== 2) {
    throw new SelectorSyntaxError(
      `invalid edge-selector term ${quoted}; expected >X, <Y, or |Z (joined with \`and\`)${widerLanguage(term)}`,
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
      `invalid edge-selector term ${quoted}; expected >X, <Y, or |Z${widerLanguage(term)}`,
      from,
      to,
    );
  }

  return { kind, axis, from, to };
}

/**
 * Every key an edge query reads, in the order a refusal lists them. A new one
 * also needs a `GRAPH_FEATURES` entry in dsl.ts: hosts through 0.0.7 drop unknown keys.
 */
export const EDGE_QUERY_KEYS: readonly string[] = [
  "generatedBy",
  "curve",
  "role",
  "adjacentTo",
  "at",
  "dihedral",
  "parallel",
  "longerThan",
  "on",
  "between",
  "not",
];

const FACE_NORMALS = ["+x", "-x", "+y", "-y", "+z", "-z"];

export type Hint = (key: string, value: unknown) => string | undefined;

/**
 * Why a query object cannot be read, naming what to write instead, or
 * `undefined` when it can: a key the query does not have, or an `at` or
 * `adjacentTo` it cannot use.
 *
 * `check_query_shape` in `selectors.rs` is the same check in the same words;
 * the `queries` in `eval/selectors.json` hold the two together.
 */
export function queryShapeError(query: Record<string, unknown>, kind: "edge" | "vertex"): string | undefined {
  const [noun, known, listing] =
    kind === "edge"
      ? ["an edge query", EDGE_QUERY_KEYS, `Its keys are ${list(EDGE_QUERY_KEYS)}.`]
      : ["a vertex query", ["at"], 'Its only key is at, e.g. { at: { z: "max" } }.'];
  const named = unknownKeys(query, known, (key, value) => queryKeyHint(key, value, known));
  if (named) return `${noun} ${named}. ${listing}`;
  return atError(query.at) ?? adjacentToError(query.adjacentTo) ?? notError(query.not, kind);
}

/** Why a `not` cannot be read: it is one query, one level deep, and not empty. */
function notError(not: unknown, kind: "edge" | "vertex"): string | undefined {
  if (not === undefined) return undefined;
  if (typeof not !== "object" || not === null || Array.isArray(not)) {
    return `an edge query's not is ${render(not)}, where it takes a query of its own, e.g. not: { parallel: "z" }`;
  }
  const inner = not as Record<string, unknown>;
  if ("not" in inner) {
    return "an edge query's not holds another not; one level is all there is, and two negations are a positive term — say that instead";
  }
  if (Object.keys(inner).length === 0) {
    return 'an edge query\'s not is empty, so it would take nothing away; give it a term, e.g. not: { parallel: "z" }';
  }
  return queryShapeError(inner, kind);
}

export function unknownKeys(object: Record<string, unknown>, known: readonly string[], hint: Hint): string | undefined {
  const unknown = Object.keys(object)
    .filter((key) => !known.includes(key))
    .sort();
  if (unknown.length === 0) return undefined;
  const named = unknown.map((key) => {
    const fix = hint(key, object[key]);
    return fix === undefined ? `"${key}"` : `"${key}" (write ${fix} instead)`;
  });
  return `has no ${unknown.length === 1 ? "key" : "keys"} ${list(named)}`;
}

const SYNONYMS: Record<string, string> = {
  tag: "on",
  tags: "on",
  feature: "on",
  features: "on",
  length: "longerThan",
  minlength: "longerThan",
  type: "curve",
  kind: "curve",
  along: "parallel",
  angle: "dihedral",
  convexity: "dihedral",
};

function queryKeyHint(key: string, value: unknown, known: readonly string[]): string | undefined {
  const normal = normalize(key);
  const field = known.find((candidate) => normalize(candidate) === normal);
  if (field !== undefined) return spelled(field, value);
  const text = typeof value === "string" ? value : undefined;
  if (["facenormal", "normal", "facing"].includes(normal) && known.includes("adjacentTo")) {
    return `adjacentTo: { faceNormal: ${render(text ?? "+z")} }`;
  }
  if (["x", "y", "z"].includes(normal)) return `at: { ${normal}: ${render(text ?? "max")} }`;
  if (normal === "top") return 'at: { z: "max" }';
  if (normal === "bottom") return 'at: { z: "min" }';
  if (normal === "count") {
    const count = typeof value === "number" && Number.isInteger(value) && value > 0 ? value : undefined;
    return count === undefined ? ".expect({ count }) on the selection" : `.expect({ count: ${count} }) on the selection`;
  }
  if (normal === "expect") return ".expect({ count }) on the selection";
  const synonym = SYNONYMS[normal];
  return synonym !== undefined && known.includes(synonym) ? spelled(synonym, value) : undefined;
}

function atError(at: unknown): string | undefined {
  if (at === undefined || at === null) return undefined;
  if (!isObject(at)) return `at is an object of axes, e.g. at: { z: "max" }, not ${render(at)}.`;
  const named = unknownKeys(at, ["x", "y", "z"], (key, value) => {
    const normal = normalize(key);
    if (["x", "y", "z"].includes(normal)) return spelled(normal, value);
    if (normal === "top") return 'z: "max"';
    if (normal === "bottom") return 'z: "min"';
    return undefined;
  });
  if (named) return `at ${named}. Its keys are x, y and z, e.g. at: { z: "max" }.`;
  for (const axis of ["x", "y", "z"]) {
    const extreme = at[axis];
    if (extreme === undefined || extreme === null || extreme === "min" || extreme === "max") continue;
    const lower = typeof extreme === "string" ? extreme.toLowerCase() : undefined;
    const fix = lower === "min" || lower === "max" ? ` (write "${lower}" instead)` : "";
    return `at.${axis} must be "min" or "max", not ${render(extreme)}${fix}.`;
  }
  return undefined;
}

function adjacentToError(adjacent: unknown): string | undefined {
  if (adjacent === undefined || adjacent === null) return undefined;
  if (!isObject(adjacent)) {
    const normal = (typeof adjacent === "string" ? faceNormal(adjacent) : undefined) ?? "+z";
    return `adjacentTo is an object, e.g. adjacentTo: { faceNormal: "${normal}" }, not ${render(adjacent)}.`;
  }
  const named = unknownKeys(adjacent, ["faceNormal"], (key, value) =>
    ["facenormal", "normal", "facing"].includes(normalize(key)) ? spelled("faceNormal", value) : undefined,
  );
  if (named) {
    return `adjacentTo ${named}. Its only key is faceNormal, e.g. adjacentTo: { faceNormal: "+z" }.`;
  }
  const normal = adjacent.faceNormal;
  if (normal === undefined || normal === null) {
    return 'adjacentTo needs faceNormal, e.g. adjacentTo: { faceNormal: "+z" }.';
  }
  if (typeof normal === "string" && FACE_NORMALS.includes(normal)) return undefined;
  const meant = typeof normal === "string" ? faceNormal(normal) : undefined;
  const fix = meant === undefined ? "" : ` (write "${meant}" instead)`;
  return `adjacentTo.faceNormal must be "+x", "-x", "+y", "-y", "+z" or "-z", not ${render(normal)}${fix}.`;
}

/** The face normal a loosely written one means: `"+Z"` and `"z"` are `"+z"`. */
function faceNormal(written: string): string | undefined {
  const lower = written.toLowerCase();
  const signed = lower.length === 1 ? `+${lower}` : lower;
  return FACE_NORMALS.find((normal) => normal === signed);
}

/** The key a hint names, with the value the author wrote when it is a string or a whole number: `on: "lip"`. */
export function spelled(key: string, value: unknown): string {
  return typeof value === "string" || Number.isSafeInteger(value) ? `${key}: ${render(value)}` : key;
}

/** A key as a person might have meant it: `generated_by` is `generatedby`. */
export function normalize(key: string): string {
  return key.replace(/[_\- ]/g, "").toLowerCase();
}

/** `a`, `a and b`, `a, b and c`. */
export function list(items: readonly string[]): string {
  return items.length < 2 ? (items[0] ?? "") : `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

export function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function render(value: unknown): string {
  return JSON.stringify(value) ?? String(value);
}

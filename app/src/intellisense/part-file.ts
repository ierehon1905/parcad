/**
 * A part, as TypeScript has to see it.
 *
 * A part is not a module and not a script: it is the body of a function whose
 * parameters are the DSL, because that is literally how it runs —
 * `new Function(...names, source)` in engine.ts, and the same in tools/run.ts.
 * Two things follow, and both are why this file exists rather than handing the
 * document straight to the language service.
 *
 * A part ends in `return`, which is an error anywhere but a function body, so
 * the text the compiler reads is wrapped. The wrapper is one line with no
 * newline of its own, and every position crosses this boundary through
 * `toCompiler` / `toDocument` — an unmapped offset lands a diagnostic or a
 * tooltip a character short of where it belongs, which looks like a bug in the
 * editor rather than in the map.
 *
 * And the DSL arrives as ambient globals rather than as an import, because
 * nothing in the document imports it. The declarations are generated from
 * `Object.keys(dsl)`, the same list the runtime binds, so a name can never be
 * offered here and missing there.
 */

/** The document, as a file the compiler will accept. */
export const PART_FILE = "/part.js";

/** The generated ambient declarations, and the module they are quoted from. */
export const GLOBALS_FILE = "/parcad-globals.d.ts";
export const DSL_FILE = "/dsl.ts";
/** How the generated declarations spell `DSL_FILE`, which is beside them. */
const DSL_MODULE = "./dsl";

const PREFIX = "function __part() {";
const SUFFIX = "\n}\n";

/** The document, wrapped in the function body a part actually runs as. */
export function compilerText(source: string): string {
  return PREFIX + source + SUFFIX;
}

/** A document offset, as the compiler numbers it. */
export function toCompiler(pos: number): number {
  return pos + PREFIX.length;
}

/**
 * A compiler offset, as the document numbers it.
 *
 * Clamped rather than returned raw: a syntax error can be reported against the
 * wrapper itself, and a negative offset is not a position the editor can mark.
 */
export function toDocument(pos: number, docLength: number): number {
  return Math.min(Math.max(pos - PREFIX.length, 0), docLength);
}

/**
 * Every DSL name, declared as the global the part sees.
 *
 * `typeof import(...)` rather than a copy of each signature: the declarations
 * carry no types of their own, so they cannot drift from the DSL and there is
 * nothing here to keep up to date when a signature changes.
 */
export function globalsFor(names: readonly string[]): string {
  return names
    .map((name) => `declare const ${name}: typeof import("${DSL_MODULE}").${name};`)
    .join("\n");
}

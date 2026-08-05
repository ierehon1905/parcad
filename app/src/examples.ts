/**
 * The example parts offered in the toolbar.
 *
 * The sources are the files in `examples/`, read at build time — not copies of
 * them. This used to be three template literals maintained by hand, which is a
 * guarantee that the editor will eventually show a script that no longer runs:
 * the same failure mode as the checked-in graph JSON that CLAUDE.md warns
 * about, one directory over. `eval/cases/` runs these same files, so an example
 * that breaks fails a case instead of surprising someone in the app.
 *
 * Adding an example is therefore adding a file to `examples/` — and, if it is
 * worth protecting, a case in `eval/cases/` that measures it.
 */

const sources = import.meta.glob("../../examples/*.js", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

export interface Example {
  /** Filename stem, e.g. `pillow-block`. Used as the `<option>` value. */
  id: string;
  /** What the picker shows. */
  label: string;
  source: string;
}

/**
 * The first three are the tutorial order — one feature each, in the order the
 * concepts build. Everything after is a real part, alphabetically, because no
 * ordering of "an M3 standoff" against "a pipe tee" means anything.
 */
const FIRST = ["bracket", "enclosure", "edge-fillets"];

function idOf(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1).replace(/\.js$/, "");
}

function rank(id: string): number {
  const i = FIRST.indexOf(id);
  return i === -1 ? FIRST.length : i;
}

export const EXAMPLES: Example[] = Object.entries(sources)
  .map(([path, source]) => {
    const id = idOf(path);
    return { id, label: id.replace(/-/g, " "), source };
  })
  .sort((a, b) => rank(a.id) - rank(b.id) || a.id.localeCompare(b.id));

export function exampleSource(id: string): string {
  return EXAMPLES.find((e) => e.id === id)?.source ?? EXAMPLES[0].source;
}

/** The part the editor opens on, and the one the browser artifact is baked from. */
export const BRACKET = exampleSource("bracket");

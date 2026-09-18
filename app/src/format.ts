/**
 * Prettier over a part script, as a save does it.
 *
 * Loaded on the first save rather than with the window: the parser and printer
 * are most of a megabyte, and nothing on screen needs them until then.
 */

/** 80 columns rewraps the seed parts' selector chains; past 100 changes nothing measurable. */
const OPTIONS = { parser: "babel", printWidth: 100 } as const;

export interface Formatted {
  source: string;
  cursor: number;
}

/** The source formatted, with `cursor` carried to where it lands — or undefined if it does not parse. */
export async function formatSource(source: string, cursor: number): Promise<Formatted | undefined> {
  const [prettier, babel, estree] = await Promise.all([
    import("prettier/standalone"),
    import("prettier/plugins/babel"),
    import("prettier/plugins/estree"),
  ]);
  try {
    const result = await prettier.formatWithCursor(source, {
      ...OPTIONS,
      cursorOffset: cursor,
      plugins: [babel, estree],
    });
    return { source: result.formatted, cursor: result.cursorOffset };
  } catch {
    // A half-typed script still saves as typed; the evaluation already shows why it is broken.
    return undefined;
  }
}

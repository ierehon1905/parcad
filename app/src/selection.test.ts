/**
 * One `user-select` in the frontend, and no more.
 *
 * The rule this protects is in `drag.ts`: text stays selectable everywhere, and
 * the only suppression is the transient one a non-text drag holds. A pane that
 * marks itself `select-none` breaks that quietly and in the damaging direction
 * — an error message or a measured number that can no longer be copied — and it
 * looks like a tidy-up in a diff. So it fails here instead. Run with
 * `bun test` from `app/`.
 */

import { expect, test } from "bun:test";
import { Glob } from "bun";

/** Where the one rule lives, and the file that explains it. */
const ALLOWED = new Set(["src/style.css", "src/drag.ts", "src/selection.test.ts"]);

/* `select-all` is the opposite move — one click takes the whole agent link —
   and stays welcome. What is forbidden is anything that takes selection away. */
const FORBIDDEN = /user-select|select-none|select-text\b/;

test("only style.css touches user-select", async () => {
  const offenders: string[] = [];
  for await (const path of new Glob("src/**/*.{ts,tsx,css,html}").scan(".")) {
    const normal = path.replaceAll("\\", "/");
    if (ALLOWED.has(normal)) continue;
    const text = await Bun.file(path).text();
    for (const [i, line] of text.split("\n").entries()) {
      if (FORBIDDEN.test(line)) offenders.push(`${normal}:${i + 1}: ${line.trim()}`);
    }
  }
  expect(
    offenders,
    "selection is suppressed only for the length of a drag: hold beginDrag() from src/drag.ts instead",
  ).toEqual([]);
});

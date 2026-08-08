/**
 * Writing a call from the palette into the part, and finding where to leave the
 * caret. Text and editor state only — no component imports this to render.
 */

import { flashInsert } from "./editor-marks";
import * as S from "./state";


/**
 * Write a call from the palette into the part.
 *
 * Two things make the difference between a palette worth pressing and one you
 * press once. Continuation lines are re-indented to the line the cursor is on,
 * so a `loft` dropped inside a function does not land against the left margin.
 * And the first argument is left *selected* rather than the caret being parked
 * after the call, because every one of these snippets carries a plausible
 * number that is not yours: the next keystroke should replace 30, not append
 * to it.
 */
export function insertSnippet(snippet: string) {
  const editor = S.editor();
  const range = editor.state.selection.main;
  const line = editor.state.doc.lineAt(range.from);
  const indent = /^[ \t]*/.exec(line.text)![0];
  const text = snippet.split("\n").join(`\n${indent}`);

  const opening = text.indexOf("(");
  const start = opening + 1;
  const end = opening < 0 ? -1 : firstArgumentEnd(text, start);
  const selected = end > start;

  editor.dispatch({
    changes: { from: range.from, to: range.to, insert: text },
    selection: selected
      ? { anchor: range.from + start, head: range.from + end }
      : { anchor: range.from + text.length },
    scrollIntoView: true,
    // In the same transaction, in post-change coordinates: the call highlights
    // itself as it lands rather than a frame later.
    effects: flashInsert.of({ from: range.from, to: range.from + text.length }),
  });
  editor.focus();

  // The fade is CSS; this only takes the decoration away once it has finished,
  // so the mark does not outlive the animation and re-appear on a later
  // re-render. A second insertion cancels the first — the newest write is the
  // one worth pointing at.
  window.clearTimeout(flashTimer);
  flashTimer = window.setTimeout(() => {
    S.editorRef.current?.dispatch({ effects: flashInsert.of(undefined) });
  }, FLASH_MS);
}

/** Long enough to catch the eye across the pane, short enough not to linger. */
const FLASH_MS = 900;
let flashTimer: number | undefined;

/**
 * Where the first argument of a call ends.
 *
 * A scan rather than a split on commas, because half these snippets have a
 * comma inside the first argument — `extrude([[-10, -5], …], 3)` — and a
 * selection that stopped at the first one would hand back a broken expression.
 * Nesting and string literals are all this has to understand; the snippets are
 * written in this file and none of them contains a regex or a comment.
 */
export function firstArgumentEnd(text: string, start: number): number {
  let depth = 0;
  let quote = "";
  for (let i = start; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === "\\") i++;
      else if (c === quote) quote = "";
      continue;
    }
    if (c === '"' || c === "'" || c === "`") quote = c;
    else if (c === "(" || c === "[" || c === "{") depth++;
    else if (c === ")" || c === "]" || c === "}") {
      if (depth === 0) return i;
      depth--;
    } else if (c === "," && depth === 0) return i;
  }
  return start;
}

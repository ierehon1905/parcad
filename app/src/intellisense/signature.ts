/**
 * Where a signature breaks, when it will not fit on one line.
 *
 * TypeScript hands back a signature already broken where *it* thinks a break
 * belongs: a nested object type arrives as its own indented block. What it
 * never breaks is the parameter list, which comes as one long line however
 * many parameters are on it — so a card that is 500px wide soft-wraps it at
 * whatever character happens to reach the edge, and `holeFor` reads as
 * `(thread: string, depth: number,` / `options?: {`. A break in the middle of a
 * parameter list is worse than no break, because the thing a reader came for
 * is the shape of the arguments.
 *
 * So the break is put where it belongs — one parameter per line — and put here
 * rather than in the rendering, on the tokens rather than on the text, so that
 * every run keeps the kind the compiler gave it and the card's colours survive
 * the rewrap.
 */

import type { Span } from "./analyzer";

/** TypeScript's own indent for the blocks it breaks, so ours matches. */
const INDENT = "    ";

/** What a signature reads as, for measuring and for tests. */
export const render = (spans: readonly Span[]) => spans.map((span) => span.text).join("");

/**
 * The same signature, with one parameter per line if it needs it.
 *
 * The rule is Prettier's, and it is all or nothing: a signature that does not
 * fit on a single line puts *every* parameter on its own. Breaking only the
 * ones that overflow is what produced `(thread: string, depth: number,` above
 * `options?: {` — an arbitrary place to stop, which reads as damage rather
 * than as structure. "Does not fit" therefore counts a signature TypeScript
 * has already broken itself: `holeFor` fits its first line in 58 of the 66
 * columns a card has and is still four lines tall, and four lines with the
 * parameters aligned beats four lines without.
 *
 * `columns` is how many characters the card can show — `card.tsx` measures it
 * off the font rather than assuming one. Unchanged when the signature is one
 * line that fits, when there is no parameter list, and when that list is
 * empty: a signature this cannot improve is one it must not touch.
 */
export function wrapSignature(spans: readonly Span[], columns: number): Span[] {
  const whole = render(spans);
  if (!whole.includes("\n") && whole.length <= columns) return [...spans];

  const list = parameterList(spans);
  if (!list || list.close === list.open + 1) return [...spans];

  const out: Span[] = spans.slice(0, list.open + 1);
  let start = list.open + 1;
  let depth = 0;

  for (let i = start; i < list.close; i++) {
    depth += nesting(spans[i]);
    if (depth === 0 && spans[i].text === "," && spans[i].kind === "punctuation") {
      out.push(...parameter(spans, start, i), { text: ",", kind: "punctuation" });
      start = i + 1;
    }
  }
  out.push(...parameter(spans, start, list.close), { text: "\n", kind: "lineBreak" });
  out.push(...spans.slice(list.close));
  return out;
}

/** One parameter, on a line of its own and indented under the bracket. */
function parameter(spans: readonly Span[], from: number, to: number): Span[] {
  const out: Span[] = [{ text: `\n${INDENT}`, kind: "lineBreak" }];
  for (let i = from; i < to; i++) {
    // The space that followed the comma is the line break now.
    if (i === from && spans[i].kind === "space") continue;
    // A block TypeScript already broke is one level deeper than it was.
    out.push({ ...spans[i], text: spans[i].text.replace(/\n/g, `\n${INDENT}`) });
  }
  return out;
}

/**
 * The parameters, among everything else in brackets.
 *
 * A quick-info line can open with a bracket that is not a parameter list at
 * all — `(method) EdgeSelection.fillet(…)` starts with one — so a group counts
 * only when what follows it is the return type: `=>` for a value whose type is
 * a function, `:` for a method or a function declaration.
 */
function parameterList(spans: readonly Span[]): { open: number; close: number } | undefined {
  for (let i = 0; i < spans.length; i++) {
    if (spans[i].kind !== "punctuation" || spans[i].text !== "(") continue;
    const close = matching(spans, i);
    if (close === undefined) continue;
    const after = render(spans.slice(close + 1)).trimStart();
    if (after.startsWith("=>") || after.startsWith(":") || after === "") return { open: i, close };
    i = close;
  }
  return undefined;
}

function matching(spans: readonly Span[], open: number): number | undefined {
  let depth = 0;
  for (let i = open; i < spans.length; i++) {
    depth += nesting(spans[i]);
    if (depth === 0 && i > open) return i;
  }
  return undefined;
}

/**
 * How much deeper a token goes.
 *
 * Angle brackets count, because `Record<string, number>` holds a comma that is
 * not a parameter boundary — but only when the token is exactly one of them,
 * which is what keeps `=>` from reading as a closing bracket and unbalancing
 * everything after it.
 */
function nesting(span: Span): number {
  if (span.kind !== "punctuation") return 0;
  if (["(", "[", "{", "<"].includes(span.text)) return 1;
  if ([")", "]", "}", ">"].includes(span.text)) return -1;
  return 0;
}

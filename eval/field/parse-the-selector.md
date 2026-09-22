---
tool:    check_selector
also:    evaluate_part
reach:   evaluate_part, check_selector
verdict: SELECTOR\s*[=:]?\s*["'`]?>Z["'`]?\s*$|SELECTOR\s*[=:]?\s*\{?\s*at:\s*\{\s*z:\s*["']max["']\s*\}\s*\}?|SELECTOR\s*[=:]?\s*\{?\s*adjacentTo:\s*\{\s*faceNormal:\s*["']\+z["']\s*\}\s*\}?
trap:    SELECTOR\s*[=:]?\s*\{\s*dihedral:\s*["']convex["']\s*\}
why:     |
  check_selector parses a selector without building anything and was never
  called in the coin-holder session, whose one selector error cost a 3 KB
  resend (docs/COIN_HOLDER_REVIEW.md, L2, L14). Descriptions are read once at
  the start; an error is read at the moment of need, so since 2026-09-18 the
  selector refusal ends by naming check_selector with the selector as its
  argument. This case measures whether that line is acted on: `reach` needs
  check_selector called, and a trial that fixes the string and re-evaluates
  without it is LUCKY, not SOUND. Three right answers: `>Z`, its query form,
  and the edges adjacent to the +z face, which on a box are the same four.
  The trap is every outside edge — what a model writes when it keeps the
  `not` in mind and drops the `>Z`.
---
Use the parcad MCP tools. Here is a script:

```js
// Round the top edges of the block: the four along its top face, not the
// four vertical ones.
const block = box(40, 30, 20).tag("block");
return block.edges(">Z and not |Z").fillet(2);
```

It does not build: the selector is not in the grammar. Find a selector that
selects what the comment asks for, check that it parses with the tool parcad's
refusal names — before you build anything with it — and then evaluate the
fixed script.

Rules: do not guess a selector and go straight to evaluating it; parse it
first with the tool the refusal points you to. End with a one-line verdict in
exactly this form:

SELECTOR = <the selector in your fixed script>

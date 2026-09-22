---
tool:    evaluate_part.refusal
reach:   evaluate_part
verdict: RENAMED\s*[=:]?\s*[A-Za-z_$][\w$]*
trap:    RENAMED\s*[=:]?\s*(clearance|none|nothing)\b
why:     |
  Every DSL export is a parameter of every script, so a local called
  `clearance` is a SyntaxError from an engine that names no identifier —
  QuickJS says "invalid redefinition of parameter name" and stops. The rule is
  in the first paragraph of read_docs `dsl` and was read, in context, by the
  session that then hit it (docs/COIN_HOLDER_REVIEW.md, L1), so this case
  measures the error message, not the documentation. Since 2026-09-18 the
  refusal names the builtin, proved by recompiling without it. A trial that
  answers `clearance` renamed nothing and read the message as a mystery; one
  that answers none inlined the number instead of keeping the script's shape.
---
Use the parcad MCP tools. Here is a script for a coin tray:

```js
const coin = 23.25; // 1 euro
const clearance = 0.6; // bore over the coin diameter
const bore = coin + clearance;
const wall = 2.4;
const depth = 12;
const tray = box(bore + 2 * wall, bore + 2 * wall, depth + 2).tag("tray");
// 12 mm deep from the top face, with 1 mm of overshoot above it.
const pocket = cylinder(bore / 2, depth + 1).at(0, 0, 1.5).tag("pocket");
return tray.cut(pocket);
```

It does not build. Make it build with the smallest change that keeps every
number and every line as it is, and say which name you changed and what you
changed it to.

Rules: change only what parcad's refusal tells you to; do not rewrite the
part, and do not remove the variable. Evaluate the fixed script before
answering. End with a one-line verdict in exactly this form:

RENAMED = <the new name of the variable you renamed>

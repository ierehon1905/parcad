---
tool:    evaluate_part.refusal
reach:   evaluate_part
verdict: ROTATE\s*[=:]?\s*\.?rotate\(\s*["']z["']\s*,\s*45\s*\)
trap:    HOST\s*[=:]?\s*too old
why:     |
  `.rotate(0, 0, 45)` is the OpenSCAD form and the single most likely wrong
  call on this surface. Until 2026-09-18 the DSL accepted it — the axis was
  the number 0, the angle 0, the 45 dropped — and the kernel refused the graph
  with a type error that ended "this host is too old for it: update it", so
  the session that hit it reported a parcad bug to its user
  (docs/COIN_HOLDER_REVIEW.md, L3). Now the DSL refuses at the call and names
  the form. The trap is the misreading that message invited: a trial that says
  the host is too old read the boilerplate and not the fault.
---
Use the parcad MCP tools. Here is a script for a plate with one corner cut
off at 45 degrees:

```js
const plate = box(60, 40, 6).tag("plate");
const cutter = box(20, 20, 8).rotate(0, 0, 45).at(30, 20, 0).tag("cutter");
return plate.cut(cutter);
```

It does not build. Fix it so the cutter is turned 45 degrees about Z, and
tell me whether the problem was in the script or in the parcad host itself.

Rules: the fix must come from what parcad's refusal says, not from a guess;
evaluate the fixed script before answering. End with two lines in exactly
this form:

HOST = <too old | fine>
ROTATE = <the rotate call in your fixed script>

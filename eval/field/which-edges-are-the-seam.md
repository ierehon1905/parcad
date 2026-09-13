---
tool: evaluate_part
reach: evaluate_part
verdict: FACES\s*[=:]\s*9
trap: FACES\s*[=:]\s*(8|10|11)\b
quote: \b9\b
why: |
  The seam where a boss meets its plate is the edge every real part needs
  rounded and the one no directional selector can name: it is at no extreme,
  and `|Z` is the wrong axis. Three selectors say it — `between` the two
  names, `dihedral: "concave"` (it is the only inside corner), or
  `generatedBy` on a tagged union — and all three are new enough that a model
  has to read the language reference to find them. The face count is what
  proves the fillet landed on that edge and nothing else: eight faces before,
  one blend face after. Ten would mean the boss's top rim went too; eight,
  that nothing was rounded and the script still evaluated.
---
Use the parcad MCP tools.

Here is a parcad script with one blank in it:

```js
const plate = box(60, 40, 10).tag("plate");
const boss = cylinder(10, 20).at(0, 0, 10).tag("boss");
return union(plate, boss)
  .edges(SELECTOR)
  .expect({ count: 1 })
  .fillet(2);
```

The boss stands on the plate, half buried. I want the seam where the boss meets the plate rounded — that one edge, nothing else.

Question: what do I write in place of SELECTOR, and how many faces does the finished part have?

Rules: the selector must be checked by evaluating the completed script with parcad, not by reasoning about what selectors look like; the face count must be the number a parcad tool reports for the finished part. If your first selector is refused, read the refusal, fix the selector, and evaluate again. End with two lines in exactly this form:

SELECTOR: <what you wrote>
FACES = <number>

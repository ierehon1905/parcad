---
tool: evaluate_part.refusal
reach: evaluate_part
verdict: RADIUS\s*[=:]?\s*3\b
trap: RADIUS\s*[=:]?\s*4\b
why: "Error messages name the fix" is a rule in CLAUDE.md, and whether a model can act on one is a separate fact from whether it is well worded. The kernel refuses this fillet with a crash message that says what class of thing went wrong and not the number, so the model has to bisect: 3 works, 4 does not, on a 4 mm plate. 4 is the trap — the radius equals the thickness and looks like it should just fit. A trial that answers 4 read the arithmetic instead of the refusal; a trial that gives up read the word "crashed" as a wall rather than as information.
---
Use the parcad MCP tools. This script does not build:

```js
const b = box(20, 20, 4).tag("b");
return b.edges(">Z and >Y and |X").expect({ count: 1 }).fillet(6);
```

Question: parcad refuses it. Find the largest whole number of millimetres you can put in that `fillet(...)` call and still have the part build.

Rules: establish it by evaluating, not by reasoning about the box's dimensions — the kernel is the authority on what it will accept and the answer is not necessarily the number you would predict. Read what the refusal tells you rather than retrying the same call. End with a one-line verdict in exactly this form:

RADIUS = <number>

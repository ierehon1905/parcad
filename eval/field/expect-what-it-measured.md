---
tool:    evaluate_part.treatments
reach:   evaluate_part
input:   \.expect\(\s*\{\s*count:\s*9\s*\}
verdict: EXPECT\s*[=:]?\s*9\s*(,|and)\s*2\b
trap:    EXPECT\s*[=:]?\s*(8|12|1)\b
quote:   \b9\b
why:     |
  The server instructions have always said to add `.expect({ count: n })` so
  a selector that drifts fails aloud, and the coin-holder session wrote 32
  scripts with fillets and never once used it: nothing told it n. Since
  2026-09-18 every treatment in an evaluate_part reply carries `edges`, the
  count the selector resolved to on the shape it ran against, measured. This
  is the authoring case for that field: `input` is load-bearing, because the
  route is a script the model sends and no sentence in its reply shows whether
  the count it wrote came from the reply. 9 is not the 8 a plate has of long
  edges: the first selector is scoped to the plate and one of its long edges
  is split by the boss. A trial that answers 8 or 12 counted the source.
---
Use the parcad MCP tools. Here is a script for a drilled plate with a boss:

```js
const plate = box(80, 50, 8).tag("plate");
const boss = cylinder(12, 14).at(20, 0, 7).tag("boss");
const body = union(plate, boss)
  .cut(cylinder(3.4, 40).at(20, 0, 0))
  .cut(cylinder(2.2, 20).at(-25, 15, 0))
  .cut(cylinder(2.2, 20).at(-25, -15, 0));
return body
  .edges({ on: "plate", dihedral: "convex", longerThan: 20 }).fillet(2)
  .edges({ on: "boss", dihedral: "convex" }).fillet(1);
```

Make the two edge selections fail aloud if a later edit changes how many
edges they pick: add `.expect({ count: n })` to each, with the n parcad
measured for that treatment when it built this part. Then evaluate the script
with the expectations in it and confirm it still builds.

Rules: each n must be a number parcad reported for that treatment, not one
you worked out from the shapes — do not count edges from the script. End with
a one-line verdict in exactly this form:

EXPECT = <n for the fillet(2)>, <n for the fillet(1)>

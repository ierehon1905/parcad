---
tool:    inspect_treatment_target
also:    evaluate_part
reach:   evaluate_part, inspect_treatment_target
verdict: LONGEST\s*[=:]?\s*24\.4\d*\b
trap:    LONGEST\s*[=:]?\s*(2|50|14)(\.0+)?\b
quote:   24\.4\d+
why:     |
  A fillet refusal lists six of its edges, shortest first, and says how many
  more there are; inspect_treatment_target lists every one without rebuilding
  and was never called in the session with four fillet failures in a row
  (docs/COIN_HOLDER_REVIEW.md, L7, L14). Since 2026-09-18 the refusal ends by
  naming it with the node as its argument. The longest edge of this target is
  one of the twenty the refusal leaves out, and its length is not in the
  script: the rib is two boxes crossed at 60°, so their 50 mm top edges are
  split where they meet. A trial answering 2 read the refusal's listing as the
  whole; 50 read the script; 14 read the rib's height.
---
Use the parcad MCP tools. Here is a script for a plate with a crossed rib:

```js
const plate = box(60, 40, 6).tag("plate");
const rib = union(box(50, 2, 14), box(50, 2, 14).rotate("z", 60)).at(0, 0, 10).tag("rib");
return union(plate, rib).edges({ on: "rib", dihedral: "convex" }).fillet(3);
```

The fillet does not build. Without changing the script, tell me the length of
the longest edge that fillet would act on.

Rules: the refusal shows only some of the edges; get the whole target from the
tool the refusal names, and take the length from what it reports — not from
the box sizes in the script. End with a one-line verdict in exactly this form:

LONGEST = <length in mm, to two decimals>

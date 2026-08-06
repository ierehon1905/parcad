---
tool: inspect_treatment_target
reach: inspect_treatment_target
verdict: 79(\.0+)?\s*mm
trap: \b8(\.0+)?\s*mm\b.{0,40}$
quote: 79
why: The corner fillet is authored as `.vertices(...).expect({ count: 1 })`, so the script's own number is 1 and its comment says "three incident edges" — a trial that quotes either looks right. What the script cannot say at all is how long those edges are, and the longest is 79 mm, not the 80 mm the plate is wide, because the chamfer before it already took a millimetre off. That millimetre is the whole case: it is only visible to a tool that resolves the target against the shape as it stands when the treatment runs.
---
Use the parcad MCP tools. The part is bracket.js in the parcad project folder.

Question: the last treatment in that script is a corner fillet. When the kernel actually runs it, which edges does it round, and how long is the longest of them?

Rules: the answer must come from a parcad tool that resolves that treatment against the real shape. Do NOT take it from the script's `.expect(...)`, from its comments, or from the plate's overall dimensions — those are what was asked for, and the question is what the kernel does. End with a one-line verdict in exactly this form:

EDGES = <count>, LONGEST = <length> mm

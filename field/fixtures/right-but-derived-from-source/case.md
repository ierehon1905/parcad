---
source: recorded — how-many-edges, haiku, reasoning arm
tool: list_entities
reach: list_entities
verdict: SELECTABLE EDGES\s*[=:]\s*122
trap: SELECTABLE EDGES\s*[=:]\s*(246|60)\b
quote: \b122\b
why: |
  list_entities *samples* — it returns 60 edges of 122 and reports the total
  beside them, in `total_edges`. A model that counts the array it was handed
  answers 60. A model that reaches for evaluate_part instead answers 246, which
  is `topological_edges` and is twice the truth: it counts each edge once per
  adjacent face, so a plain cube reads 24. Both wrong numbers are more
  plausible than the right one and neither tool says the other exists. This
  case is here because that disagreement is invisible from inside either tool
  and only a caller asked to commit to one number ever runs into it.
---
Use the parcad MCP tools. The part is extrusion-2020.js in the parcad project folder.

I am about to write an edge selector for this part and I need to know how big the set I am selecting from is.

Question: how many edges does this part have that I could pick out with a selector?

Rules: the number must come from a parcad tool, not from counting features in the script and not from looking at a picture. Be careful of two things: it is a long extruded profile, so there are more edges than a picture suggests, and a tool may hand you a *sample* of them rather than all of them. End with a one-line verdict in exactly this form:

SELECTABLE EDGES = <number>

---
tool: evaluate_part
reach: read_docs, evaluate_part
verdict: FACES\s*[=:]\s*14\b[\s\S]*VOLUME\s*[=:]\s*(199[89](\.\d+)?|2000(\.0+)?)\s*mm
trap: FACES\s*[=:]\s*(?!14\b)\d+
quote: \b(199[89]\.\d+|2000(\.0+)?)\s*mm
why: |
  Arcs in a section are the thing a model used to fake: with only straight
  edges, a round end is a polygon of many short sides, which builds, measures
  plausibly and is wrong. This asks for a gasket whose outline and slot both
  have round ends, and checks the two numbers only true arcs give. The
  corner radii and the slot's round ends take and give back the same 25(4 − π)
  + 200 + 25π = 300 mm² between them, so the exact solid is 4 × (800 − 300) =
  2000 mm³, which evaluate_part's mesh reads to the hundredth; and it has exactly
  14 faces — top, bottom, 4 flat and 4 cylindrical outer walls, 2 flat and 2
  cylindrical slot walls. A polygon stand-in reads another face count and
  another volume (the trap); a fillet on a box gives the same outer faces and
  is also honest, so only the count and the volume are held. The route is
  read_docs to find the section vocabulary and evaluate_part to measure.
---
Use the parcad MCP tools.

I need a flat gasket to print: a 40 mm × 20 mm plate, 4 mm thick, with all
four corners rounded to a 5 mm radius, and a slot through its middle that is
30 mm long overall and 10 mm wide with fully round ends (half circles), running
along the plate's long direction.

Draw the outline and the slot as curves — real arcs, not polygons of short
straight edges — and evaluate the part.

Question: how many faces does the built part have, and what is its volume?

Rules: find how to draw arcs in the language reference rather than guessing,
and every number must come from a parcad tool that measured the built part.
End with two lines in exactly this form:

FACES = <number>
VOLUME = <number> mm

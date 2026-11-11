---
tool:    evaluate_part.between_bodies
also:    read_project, list_projects
reach:   evaluate_part
verdict: JAR\s*[=:]\s*\**jar3e\b[\s\S]*SHELF\s*[=:]\s*\**shelf4\b[\s\S]*DEPTH\s*[=:]\s*\**4\.03[0-9]*\s*mm
trap:    JAR\s*[=:]\s*\**NONE\b
quote:   \b4\.034\b
why: |
  A part in many bodies, and the reply that used to hide it. The plant-shelves
  session (2026-11-08..11) lost 9 of 32 evaluate_part and edit_part replies to
  Claude Code's limit — "result (87,631 characters) exceeds maximum allowed
  tokens" — because `between_bodies` listed every one of 465 pairs. The part
  is eval/scripts/pantry-rack.js seeded as `pantry-rack`: 7 bodies of rack and
  24 jars as references, 31 bodies, and at 0.0.24 an 87,000-character reply
  the client hides. Most jars lean a few degrees, from a table; exactly one
  pair interferes: jar3e, 208 tall and leaning 9°, reaches 4.034 mm into
  shelf4. Every height is under the 214 mm between shelves, so a reader of
  the script who skips the lean answers NONE — the trap — and one who does
  the trigonometry gets 4.33, where the rim tops out, which is not the depth
  the kernel measures. The quote is 4.034, which only `between_bodies`
  states. A first version named its one tipped jar in a constant, and every
  trial before the budget read the culprit off the script and rebuilt just
  that pair with `edits`: the table is so the script points at no jar. Seed
  the part with its header comment cut to its first line: the rest names the
  jar.
---
Use the parcad MCP tools.

The project folder holds a part called `pantry-rack`: a rack of four shelves
with jars standing on them, the jars drawn as reference bodies.

Question: is any jar drawn into a shelf, so that it would not fit where it
stands? If so, which jar, which shelf, and how far does the jar reach into
that shelf?

Rules: every number must come from a parcad tool that measured the built
part, not from the dimensions written in the script. End with three lines in
exactly this form:

JAR = <body name, or NONE>
SHELF = <body name, or NONE>
DEPTH = <number> mm

---
tool: evaluate_part
reach: read_docs, evaluate_part
verdict: THREAD\s*[=:]\s*MODELLED\b[\s\S]*FIT\s*[=:]\s*CLEAR\b[\s\S]*CLEARANCE\s*[=:]\s*0\.(19[5-9][0-9]*|20*)\s*mm
trap: CLEARANCE\s*[=:]\s*0\.40*\s*mm
quote: \b0\.200\s*mm
why: |
  A printed thread is the thing the language could not make until
  threadedRod and threadedHole, and holeFor's tapped drill is still the first
  thing a model finds under "thread". This asks for a bolt and nut that turn
  off the printer, so the honest route is read_docs for the thread functions,
  a two-body part, and evaluate_part's between_bodies for the gap. It tests
  three readings at once: that the docs lead to a modelled thread rather than
  a tap drill (THREAD = MODELLED); that the phase rule in the docs is read,
  because a nut placed a fraction of a pitch off the bolt reads interfering;
  and that the gap is quoted as measured. Two parts given 0.2 mm each sit
  0.2 mm apart across the flanks (0.19976 with the lead angle, reported as
  0.200) and 0.4 mm at crest and root, which is the trap: the number a model
  gets by doubling the clearance instead of measuring.
---
Use the parcad MCP tools.

I want to 3D-print an M6 bolt and a nut that screws onto it: a real modelled
thread on both, not a plain hole, with 0.2 mm of clearance on each part so
they turn freely. The bolt needs at least 16 mm of thread and the nut can be
a 10 mm hexagonal or square block, 5 mm thick, sitting on the bolt's thread.

Write the part as one script with the bolt and the nut as two separate
bodies, placed where the nut is screwed on, and evaluate it.

Question: is the nut clear of the bolt where you placed it, and what is the
smallest gap between the two in millimetres?

Rules: find the thread functions in the language reference rather than
guessing names, and every number must come from a parcad tool that measured
the built part — not from the clearance you asked for. End with three lines in
exactly this form:

THREAD = <MODELLED or TAP DRILL>
FIT = <CLEAR or TOUCHING or INTERFERING>
CLEARANCE = <number> mm

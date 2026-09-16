---
source: recorded — how-close-to-the-involute, haiku, non-reasoning arm, 2026-09-16; image data, the client's `tool_use_result` copies and machine paths removed
tool: evaluate_part.curve_bound_mm
reach: read_docs, evaluate_part
input: \bspurGearOutline\s*\(|\bcurve\s*:
verdict: FLANK BOUND\s*[=:]\s*(?:0\.000\d*[1-9]\d*|[1-9](?:\.\d+)?\s*(?:e|[x×]\s*10\^?)\s*[-−]\s*0?[4-9])\s*mm\W+CERTIFIED
trap: FLANK BOUND\s*[=:]\s*(?:0(?:\.0+)?|0\.001|0\.01|1e-0?3)\s*mm|ESTIMATED\s*$
why: |
  The right answer, with read_docs called twice — and both replies were over
  the client's 50,000-character limit, so each reached the model as a
  `<persisted-output>` note and a 2 KB preview of a file it had no tool to
  open. It found the gear helper by reading a seeded part's source instead.
  Counting the call as reaching read_docs graded this SOUND; the rule is that
  a hidden reply is not a read, so it is LUCKY, and `hidden` names both calls.
---
Use the parcad MCP tools.

Model a spur gear: module 2, 20 teeth, 20° pressure angle, 10 mm thick, its
tooth flanks true involutes of the base circle, each drawn within 0.001 mm of
the involute. Build it and evaluate it.

Question: how far, at worst, can the built tooth flanks be from the true
involute — anywhere along them, not only at particular points — and is that
number proven or only estimated?

Rules: find how the language draws a curve from its formula in its reference
rather than sampling points yourself, and the number must be read from a tool
reply — not the tolerance you asked for, not the mesh resolution. End with a
one-line verdict in exactly this form:

FLANK BOUND = <number> mm, <CERTIFIED or ESTIMATED>

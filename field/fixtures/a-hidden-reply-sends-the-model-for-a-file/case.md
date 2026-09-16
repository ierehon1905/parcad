---
source: recorded — how-close-to-the-involute, sonnet, effort high, 2026-09-16; image data, the client's `tool_use_result` copies and machine paths removed
tool: evaluate_part.curve_bound_mm
reach: read_docs, evaluate_part
input: \bspurGearOutline\s*\(|\bcurve\s*:
verdict: FLANK BOUND\s*[=:]\s*(?:0\.000\d*[1-9]\d*|[1-9](?:\.\d+)?\s*(?:e|[x×]\s*10\^?)\s*[-−]\s*0?[4-9])\s*mm\W+CERTIFIED
trap: FLANK BOUND\s*[=:]\s*(?:0(?:\.0+)?|0\.001|0\.01|1e-0?3)\s*mm|ESTIMATED\s*$
why: |
  The client's other way of hiding a reply: past its token cap Claude Code
  answers `Error: result (61,261 characters) exceeds maximum allowed tokens`
  and saves the result with no preview at all. The model, with no file tool,
  went looking for one (PowerShell, Grep, Read) and so strayed, which is VOID.
  `hidden` must name read_docs here too, so a round shows why the trial
  wandered.
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

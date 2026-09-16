---
tool: evaluate_part.curve_bound_mm
reach: read_docs, evaluate_part
input: \bspurGearOutline\s*\(|\bcurve\s*:
verdict: FLANK BOUND\s*[=:]\s*(?:0\.000\d*[1-9]\d*|[1-9](?:\.\d+)?\s*(?:e|[x×]\s*10\^?)\s*[-−]\s*0?[4-9])\s*mm\W+CERTIFIED
trap: FLANK BOUND\s*[=:]\s*(?:0(?:\.0+)?|0\.001|0\.01|1e-0?3)\s*mm|ESTIMATED\s*$
why: |
  A curve given by a formula cannot cross into the kernel, so the script
  draws it and states how far the drawing may be from the formula:
  curve_bound_mm, with curve_bound saying whether that is proven from the
  function's own derivatives or only read off samples. This asks for that
  bound on a gear's involute flanks. Three other numbers in the same reply
  are plausible and wrong: the tolerance asked for (0.001, an upper limit,
  not what was achieved), deviation_mm (the kernel's reading at a few points
  of the involute, 0.0 at the micron the reply rounds to — a measurement at
  points, not a bound between them), and resolution_mm (0.01, the mesh). The
  route is read_docs to find the gear helper or the curve entry, and
  evaluate_part to read the bound the reply carries. The bound depends on the
  tolerance the model passes — 0.000008 at the helper's default, 0.000114 at
  0.001 — so the verdict takes any bound under the tolerance that is not the
  tolerance itself, and the `curve bound` read in field.toml checks the
  number is the one the reply carried.
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

---
tool:    evaluate_part.checks
also:    read_project, list_projects
reach:   evaluate_part
input:   ^(?!.*"script": "(?:[^"\\]|\\.){800,}")(?=.*"project": "catch-clip")(?=.*"edits").*
verdict: CHECKS\s*[=:]\s*failed.*BITE\s*[=:]\s*16(?:\.0+)?
trap:    CHECKS\s*[=:]\s*passed
quote:   \b16(?:\.0+)?\s*mm
why:     |
  Checks that live in the part (docs/COIN_HOLDER_REVIEW.md, Appendix C §2),
  read from the reply of an `evaluate_part { project, edits }` call. The
  part is eval/scripts/checked-clip.js seeded as `catch-clip`: its catch
  reaches 1.8 mm into the latch, 72 mm³ of bite against a check asking for
  30. With REACH at 0.4 the bite is 0.4 × 10 × 4 = 16 mm³, `between_bodies`
  still says `interfering`, and only the part's own check says the catch is
  no longer caught — a model that reads the pair's verdict and not the
  check's answers PASSED, the trap. `input` requires the what-if route: a
  call carrying `project` and `edits`, and no whole script — the part is
  under 1 KB, so the guard is 800 bytes here rather than the 2 KB of the
  retainer cases. Nothing is written by the route, so trials may run together.
---
Use the parcad MCP tools.

The saved part `catch-clip` is a latch with a catch that bites into it; the
catch's reach is the constant `REACH`, 1.8 mm, and the part carries its own
checks, one of which is that the catch bites the latch. If REACH were 0.4 mm
instead, would the catch still be caught according to the part's own checks?

Measure it with parcad: build the part with that one change and read the
verdict of the part's checks from the reply. Do not change the saved part —
nothing may be written to disk. Do not work it out from the numbers in the
script.

End with a one-line verdict in exactly this form:

CHECKS = <passed or failed> · BITE = <mm³ the two bodies share> mm³

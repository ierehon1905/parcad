---
source: synthetic — the verdict pattern is the stale one this case really carried
tool: list_entities
reach: list_entities
verdict: (?i)\bno\b|does not|doesn't|clear of|misses
why: |
  Synthetic, and the only fixture whose expected outcome is a refusal. A bare
  `(?i)` is legal on its own and illegal once the negation detector wraps it, so
  a rubric like this looks fine in the case file and fails at scoring time —
  after the trials have been paid for. It used to be a traceback forty frames
  deep that took the whole suite's table with it; it must stay a message naming
  the offending pattern. Write `(?i:...)` around the part that needs it.
---
Does the blend reach the bolt circle?

Answer CLEAR or FOULED.

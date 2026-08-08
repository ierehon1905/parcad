---
source: synthetic — a case file that opens straight into its prompt, as one shipped
tool: list_entities
reach: list_entities
why: |
  The second fixture whose expected outcome is a refusal, and the one that costs
  the most when it is not. A case with no `verdict` is not a case the scorer
  half-grades: `hit(None, ...)` returns None, `not None` is true, and every
  trial grades WRONG before the reply is read. Nothing about the output says so
  — a column of WRONG is exactly what a genuinely failing case looks like, which
  is why change-the-open-part.md ran that way unnoticed while it was the only
  case driving get_session, open_project and set_script, and while the coverage
  table called all three UNTESTED.

  This fixture keeps its header and drops only the verdict, which is the weaker
  of the two shapes the mistake takes — the real one had no `---` at all, so
  `rubric()` returned {} and every field went missing at once. A fixture cannot
  reproduce that and still record its own `source`, and it does not need to: the
  guard reads `verdict` alone, so the header-less case fails through this same
  path. Scoring is free to rerun and the trials are not, so the refusal belongs
  at scoring time, naming the case.
---
Does the blend reach the bolt circle?

Answer CLEAR or FOULED.

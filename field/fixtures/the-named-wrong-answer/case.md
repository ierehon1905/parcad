---
source: synthetic
tool: list_entities
reach: list_entities
verdict: SELECTABLE EDGES\s*[=:]\s*122
trap: SELECTABLE EDGES\s*[=:]\s*(246|60)\b
quote: \b122\b
why: |
  Synthetic. The plausible wrong answer a different tool of ours hands out,
  committed to positively. WRONG, and `trap` HIT — a suite should name the wrong
  answer it expects rather than only its absence.
---
How many edges does this part have that I could pick out with a selector?

SELECTABLE EDGES = <number>

---
source: synthetic
tool: list_entities
reach: list_entities
verdict: SELECTABLE EDGES\s*[=:]\s*122
trap: SELECTABLE EDGES\s*[=:]\s*(246|60)\b
quote: \b122\b
why: |
  Synthetic. The right verdict appears in the reasoning and is then talked out
  of; the reply commits to the wrong one. Only the tail is searched, because a
  verdict is often an ordinary English word and a reply is long — a trial that
  answered FLAT FLOOR once scored a clean OPEN off "the port cavities don't open
  directly into the gallery".
---
How many edges does this part have that I could pick out with a selector?

SELECTABLE EDGES = <number>

---
tool:    evaluate_part.print_check
also:    read_project
reach:   evaluate_part
verdict: UNSUPPORTED\s*[=:]\s*\**6[56](\.[0-9]+)?\s*mm(2|²)?\s+(at|on|in)\s+\**`?lip`?
trap:    UNSUPPORTED\s*[=:]\s*\**0\b|(?i)\bno (unsupported|overhang)
quote:   \b65\.[0-9]\d*\b
why: |
  The part prints flat, is watertight, stands on one patch over its whole
  footprint and fits every bed -- every printability number parcad had before
  this feature is green on it, which is the point. Its eight retaining lips
  each reach 1.5 mm straight inward at 0 degrees, unsupported, and nothing in
  the source names an angle. A render does not settle it: the lips are inside
  the cups and an iso view shows their top surface, not their underside; a
  section shows two of eight. The trap is the answer every earlier version of
  this reply supported -- that a part which sits flat and needs no raft has no
  overhangs. `lip` is a tag, so the second half of the verdict is also a test
  of whether the places list is read rather than a total quoted.

  Written before the feature existed (docs/COIN_HOLDER_REVIEW.md, workflow
  §11) and pinned from the first build: each lip's exposed underside is the
  box inside the bore circle, 8.2428 mm² by the segment integral, 65.942 mm²
  over eight, which the mesh reads as 65.797 (chords against the bore) in
  `print_check.bodies[0].unsupported_mm2` and in the overhang
  flag that names `lip`. Nothing bridges, and the support prism to the bed
  through the plate is 791.31 mm³. The part is eval/scripts/coin-cups.js
  seeded as `coin-cups`.
---
Use the parcad MCP tools. The part is `coin-cups` in the parcad project folder.

I am printing this on an FDM printer with no support material, as drawn.

Question: how much of this part's surface is unsupported, and which named
feature is it?

Rules: the number must come from a parcad measurement, not from counting
features in the source and not from reading it off a picture. If anything you
measured was not the finished part, say so. End with a one-line verdict in
exactly this form:

UNSUPPORTED = <number> mm2 at <tag>

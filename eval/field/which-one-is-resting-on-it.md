---
tool:    evaluate_part.between_bodies
also:    read_project, list_projects
reach:   evaluate_part
verdict: SEATED\s*[=:]\s*\**LID\b[\s\S]*CONTACT\s*[=:]\s*\**537\.2[0-9]*\s*mm
trap:    SEATED\s*[=:]\s*\**(BOTH|BALL)\b
quote:   \b537\.2[0-9]*\b
why: |
  `touching` is the same word for a lid seated on a whole ring and for a ball
  resting on a floor at one point, and the coin-holder session read the second
  as the first — one repeated `closest_mm` at the far +x end of the part,
  written up as "the three touch without overlapping" and used as evidence
  that a stack seats (docs/COIN_HOLDER_REVIEW.md §2.3). The part is
  eval/scripts/rim-and-ball.js seeded as `rim-cup`: both pairs are `touching`
  with clearance 0, and only `contact_mm2` tells them apart — 537.212 mm² in
  one patch for the lid, 0.000 in none for the ball. A trial that reads the
  verdict alone has no way to choose and says both, the trap. The part is
  seeded with its comments cut down to one line — the script in `eval/`
  states both numbers, and a seeded part that names its own answer is how
  the most instructive LUCKY on record happened. 537.212 is π(30²−27²), so
  the number is still derivable by arithmetic, and that is this case's
  weakness: every before-round trial derived it and wrote 537.5, 537.17 or
  134.3, but two after-round trials derived it, rounded to 537.2 and graded
  LUCKY on a number they never measured. Read `reach` here rather than
  `sound` — the string `537.212` appears in none of the eight before
  transcripts and only in the one after trial that called `evaluate_part`.
  A better version of this case wants a contact with no closed form; this
  one has π(30²−27²).
---
Use the parcad MCP tools.

The project folder holds a part called `rim-cup`: a cup, a lid, and a ball
that sits inside it.

Question: of the lid and the ball, which one is actually resting on the cup —
bearing on a surface rather than meeting it at a point or an edge — and how
much surface does it rest on?

Rules: every number must come from a parcad tool that measured the built
part, not from the dimensions written in the script. End with two lines in
exactly this form:

SEATED = <LID or BALL or BOTH>
CONTACT = <number> mm²

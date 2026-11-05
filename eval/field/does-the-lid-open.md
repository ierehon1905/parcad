---
tool:    evaluate_part.checks
also:    read_docs
reach:   evaluate_part|edit_part
input:   turns\\?"?\s*:\s*\{
verdict: OPENS\s*[=:]\s*\**YES\b[\s\S]*CLEARANCE\s*[=:]\s*\**0\.40*\s*mm
trap:    CLEARANCE\s*[=:]\s*\**0\.(?:2|3)0*\s*mm
quote:   \b0\.40*\s*mm
why: |
  The first motion check, read by a model (docs/NEXT.md, check_motion;
  docs/PERCEPTION.md §24). The part is the hinged box drawn open and flat,
  handed over without its checks; the question is one no posed copy answers
  honestly — does the lid clear the base at every angle up to 90°, and by
  how little. `input` requires the route: a `turns` check in the script the
  model sent, which only exists after the change, so a trial on a before
  host can be LUCKY at best; the lid is the only body that can turn, so the
  key alone is the evidence. The right answer is 0.4 mm, the pin in its
  bore, held at every angle; 0.2 and 0.3 are the trap, the shut pose's
  clearance and the latch gap, which a model confusing the fold with the
  shut lid quotes. Measured 2026-11-05 on Haiku 5.5, 4 trials per arm,
  before and after: 0/8 SOUND before — every trial posed copies in 2–10°
  steps, 2–17 calls and 4.5–6.7 KB of script per trial, three 55 KB replies
  hidden and one trial left with nothing (VOID), all answering 0.4 by luck;
  7/8 after, every trial writing the `turns` check unprompted in 1–3 calls
  and ~1 KB, the eighth also writing it and marked only for "the script's
  comment says negative angles close the lid", a citation the grader does
  not read past.
---
Use the parcad MCP tools.

Here is a parcad script for a rugged case with a lid on a print-in-place
hinge, drawn open and flat, the lid folded 180° away from the base. The lid
turns about the x axis through the hinge pin at (0, -28, 16.2); negative
angles close it.

```js
const W = 107, D = 47, AY = -28, AZ = 16.2, LY = -56;
const wallCut = box(102, 42, 15);
let base = box(W, D, 16).at(0, 0, 8).cut(wallCut.at(0, 0, 9.5));
base = base.cut(box(20, 1.3, 2).at(0, 23.35, 10.5).tag("groove"));
const sx = [-43, -21.5, 0, 21.5, 43];
for (const i of [0, 2, 4]) {
  base = base.union(box(21, 10, 16).at(sx[i], -27, 8), cylinder(4, 21).rotate("y", 90).at(sx[i], AY, AZ));
}
base = base.cut(cylinder(1.9, 110).rotate("y", 90).at(0, AY, AZ));
let lid = box(W, D, 16).at(0, LY, 8).cut(wallCut.at(0, LY, 9.5));
for (const i of [1, 3]) {
  lid = lid.union(box(21, 10, 16).at(sx[i], -29, 8), cylinder(4, 21).rotate("y", 90).at(sx[i], AY, AZ));
}
lid = lid.union(cylinder(1.5, 106).rotate("y", 90).at(0, AY, AZ));
lid = lid.union(box(18, 4, 16).at(0, -80, 8), box(18, 2, 8).at(0, -81, 19.5), box(16, 1.2, 1.6).at(0, -79.6, 21.9).tag("bead"));
return { base, lid };
```

Question: does the lid open to 90° — fold from 0° to -90° about that axis —
without touching the base at any angle on the way, and what is the closest
it comes to the base anywhere in that swing, in millimetres?

Rules: the answer must come from a parcad tool that measured the lid against
the base over the whole swing, not from one or two posed copies and not from
the numbers in the script. Do not change the part's geometry. End with two
lines in exactly this form:

OPENS = <YES or NO>
CLEARANCE = <number> mm

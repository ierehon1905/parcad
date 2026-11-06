---
tool:    evaluate_part.checks
also:    read_docs
reach:   evaluate_part|edit_part
input:   pushes\\?"?\s*:\s*\[
verdict: CONTACT\s*[=:]\s*\**NO\b[\s\S]*GAP\s*[=:]\s*\**0\.2(?:4[6-9]\d*|50*)\s*mm
trap:    GAP\s*[=:]\s*\**0\.(?:10*|150*)\s*mm
quote:   \b0\.2(?:4[6-9]\d*|50*)\s*mm
why: |
  The first mechanism check, read by a model (docs/NEXT.md, mechanisms). Two
  bodies move together on one drive: a lever drops 18° on its pin and pushes
  a slider whose travel the user wrote as 30·sin θ, which misses the lever
  corner's own drop of 3(1 − cos θ). The question is whether the push holds
  contact within 0.15 mm over the whole stroke; no single posed copy answers
  it, and the honest route is a `mechanism` with a `pushes` check, which only
  exists after the change, so `input` requires the `pushes` key and a trial
  on a before host can be LUCKY at best. The right answer is NO, the gap
  opening to 0.1 + 3(1 − cos 18°) = 0.247 mm at the bottom of the stroke
  (eval/cases/lever-push-opens.json measures the same). 0.1 and 0.15 are the
  trap: the drawn gap, which a model that measured only the rest pose
  quotes, and the tolerance itself. Measured 2026-11-06 on Haiku 5.5, 4
  trials per arm, before and after: 0/8 SOUND before. Every trial read Check,
  TurnCheck and SlideCheck, built once with one body moving at a time, and
  said the tools cannot drive the two together. Seven declined to give a
  number, and one was LUCKY. After, 6/8 were SOUND, and 7/8 on a longer
  Mechanism entry the same day. Every trial read Mechanism (the
  evaluate_part description names it), sent one `mechanism` with a `pushes`
  check, built once or twice in 3–4 calls, and quoted 0.247 mm. The misses
  had the same route and number but put the two lines at the top of the reply,
  outside the tail. The Mechanism example is this lever with the corrected
  law, and two trials named its 3(cos θ − 1) term as the cause, unprompted.
---
Use the parcad MCP tools.

Here is a parcad script for a lever on a pin that pushes a slider, drawn at
rest. The lever turns about the y axis through the pin at (0, 0, 40); as it
drops from 0° to -18°, its lower end pushes the slider along +x. I have made
the slider travel 30·sin(θ) mm when the lever is θ degrees down.

```js
const PIN = [0, 0, 40];
const base = box(90, 30, 4).at(0, 0, 2);
const lever = box(6, 10, 30).at(0, 0, 25);
const slider = box(20, 10, 8).at(13.1, 0, 14);
return { base, lever, slider };
```

Question: with the lever and the slider moving together like that, does the
lever stay in contact with the slider over the whole drop — never more than
0.15 mm apart and never overlapping it by more than 0.15 mm — and what is the
widest gap between them anywhere in the stroke, in millimetres?

Rules: the answer must come from a parcad tool that measured the lever
against the slider over the whole stroke with both of them moving, not from
one or two posed copies and not from your own arithmetic. Do not change the
part's geometry or the slider's travel. End with two lines in exactly this
form:

CONTACT = <YES or NO>
GAP = <number> mm

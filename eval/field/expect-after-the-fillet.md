---
tool:    evaluate_part.treatments
reach:   evaluate_part
input:   \.fillet\(\s*4\s*,\s*\\"\|Z\\"\s*\)\s*\.expect\(
verdict: EXPECT\s*[=:]?\s*\**\s*4\s*(,|and)\s*12\b
trap:    EXPECT\s*[=:]?\s*\**\s*4\s*(,|and)\s*(4|8|16)\b
quote:   \b12\b
why:     |
  Five of 72 head-to-head ParCAD trials, all Opus or Sonnet, wrote the count
  after the treatment — `box(…).fillet(4, "|Z").expect({ count: 4 })` — where
  the reply reports it, and got "not a function", the one error in that run
  that named no fix. Since the fix `.expect` straight after a fillet or
  chamfer checks that treatment. The script is written in the shorthand
  `.fillet(r, selector)` form, which has no `.edges()` to hang an
  expectation on, so after the call is where a model reaches. `input` is
  that spelling: a trial that went straight to `.edges(sel).expect()` never
  met the change and grades LUCKY. Read `err` and the call count beside the
  grade — before the fix a trial that recovers is still SOUND, one refusal
  later. 12 is the chamfer's count on the top face after the corners are
  rounded (4 straight, 4 arcs, 4 holes); 8 leaves out the holes.
---
Use the parcad MCP tools. Here is a script for a mounting plate:

```js
const plate = box(80, 50, 6);
const hole = cylinder(3.4, 20);
return plate
  .cut(hole.at(-30, -15, 0), hole.at(30, -15, 0), hole.at(-30, 15, 0), hole.at(30, 15, 0))
  .fillet(4, "|Z")
  .chamfer(0.5, ">Z");
```

Make both edge treatments fail aloud if a later edit changes how many edges
they pick: add `.expect({ count: n })` to each one, with the n parcad measured
for it when it built this part. Then evaluate the script with the expectations
in it and confirm it still builds.

Rules: each n must be a number parcad reported for that treatment, not one you
worked out from the shapes — do not count edges from the script. End with a
one-line verdict in exactly this form:

EXPECT = <n for the fillet>, <n for the chamfer>

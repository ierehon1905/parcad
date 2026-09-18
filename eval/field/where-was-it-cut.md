---
tool:    evaluate_part.section
reach:   evaluate_part
arg:     section
input:   \boffset\b
verdict: CUT AT\s*[=:]?\s*-38(\.0+)?\b
trap:    CUT AT\s*[=:]?\s*0(\.0+)?\b
quote:   \bat_mm\b
why:     |
  The section argument is { axis, at_mm, keep }, and `offset` is what every
  other CAD tool calls at_mm. Until 2026-09-18 a host dropped the unknown key,
  cut through the middle of the part, and said so only in the reply's resolved
  `section` — on this bar that is at_mm: 0.0, and cut_fraction 1.0 either way,
  so the picture does not give it away. Now the call is refused with the field
  that was meant. SOUND needs the model to report the plane the reply says was
  cut, not the one it asked for; the trap is the middle, which is what a trial
  answers when it reads the resolved section honestly on a host that dropped
  the argument, and what it answers by accident when it never reads it at all.
  `input` is load-bearing: the first round showed most trials quietly writing
  at_mm from the schema and never sending `offset`, which answers the question
  without touching the hazard, so a trial that never sent it grades LUCKY.
---
Use the parcad MCP tools. Here is a part:

```js
const bar = box(100, 30, 20);
const pocket = cylinder(6, 12).at(-38, 0, 11);
return bar.cut(pocket);
```

Earlier I asked evaluate_part for a picture of it cut open, with `views:
["left"]` and `section: { axis: "x", offset: -38 }`, so that the cut would pass
through the pocket. Make that same request now, exactly as written, and tell me
where the plane was actually cut. If the cut did not land at x = -38, get it
there.

Rules: the answer must be the plane the reply itself reports in its resolved
`section`, not the number you asked for; do not derive it from the script. End
with a one-line verdict in exactly this form:

CUT AT = <at_mm from the reply>

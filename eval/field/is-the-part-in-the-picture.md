---
tool:    evaluate_part.views
reach:   evaluate_part
arg:     views
verdict: RULER\s*=\s*20(\.0+)?\s*mm[\s\S]*FRAMED ON\s*=\s*part\b
trap:    RULER\s*=\s*100(\.0+)?\s*mm
quote:   \b20(\.0+)?\s*mm\b
why:     |
  A part returned beside the wall it mounts on and the broom it holds, both
  `.reference()` and both far larger than the part. Until 2026-11-05 every
  view was framed on everything drawn, so the 300 mm wall filled the frame,
  the reply's `scale_mm` read 100, and the 36 mm bracket was a few pixels —
  hidden behind the wall's back face in the iso view at that. Now the frame
  fits the part alone, the ruler is 20 mm, the wall is drawn behind the part
  and cut off at the frame's edge, and the view carries `framed_on` saying
  so. SOUND needs both answers read off the view in the reply; the trap is
  the 100 mm rule the old frame carried, and no reading of the script gives
  20, which is why the ruler is the quote.
---
Use the parcad MCP tools. Here is a part: a broom holder, returned beside the
wall it screws to and the broom it holds, both drawn as references.

```js
const part = box(36, 4, 80).at(0, 2, 0).union(cylinder(12.7, 30).at(0, 14.4, 0)).cut(cylinder(10.2, 32).at(0, 14.4, 0));
const broom = cylinder(10, 300).at(0, 14.4, 0).reference();
const wall = box(300, 20, 300).at(0, -10, 0).reference();
return { part, broom, wall };
```

Build it with evaluate_part and ask for one picture, `views: ["iso"]`. Tell
me what the ruler drawn along the bottom of that picture is worth, and which
bodies the picture is framed on.

Rules: both answers must come from the view the reply describes — its
`scale_mm` and its `framed_on` — not from the script and not from looking at
the picture. End with two lines in exactly this form:

RULER = <scale_mm> mm
FRAMED ON = <the bodies framed_on names, comma separated>

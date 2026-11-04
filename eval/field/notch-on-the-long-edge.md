---
tool:    evaluate_part.checks
also:    read_docs
reach:   evaluate_part|edit_part|save_project
input:   feature\s*:\s*\{[^}]*(?:size\s*:\s*\[\s*10\s*,\s*8\b|center\s*:\s*\[\s*0(?:\.0+)?\s*,\s*-?21\b)
verdict: NOTCH CENTRE\s*[=:]\s*\**\s*[(\[]?\s*[-−]?0(?:\.0+)?\s*(?:mm)?\s*,\s*[-−]?21(?:\.0+)?\b[\s\S]*STATUS\s*[=:]\s*\**DONE
trap:    NOTCH CENTRE\s*[=:]\s*\**\s*[(\[]?\s*[-−]?36\b
quote:   [-−]?21(?:\.0+)?\b
why: |
  Requirements first (docs/NEXT.md, New capabilities 2). The head-to-head's
  notch brief, and the misread Haiku 4.5 shipped outside ParCAD: the notch
  cut on a 50 mm edge instead of an 80 mm one (research_notes/hands-on/h2h).
  The colleague's script makes exactly that mistake — the notch's faces span
  x 32..40, y −5..5, centre (36, 0) — and builds a valid part of the right
  volume and the right overall size, so neither the volume nor the bounding
  box nor print_check says anything is wrong. A notch on an 80 mm edge is
  centred at (0, ±21). `input` is a check carrying the request's notch: a
  `feature` check on the tag with the requested size [10, 8, …] or centre
  [0, ±21, …], which exists only after the change; a trial that found and
  fixed the misread by reading the script grades LUCKY (right, but by
  arithmetic on the source, with nothing in the part to hold it). Measured
  2026-11-04 on Haiku 5.5, 4 trials per arm: 0/8 SOUND before and 0/8
  after, every trial LUCKY on both hosts — each found the misread in
  `tag_extents`, fixed it, and added no check to a script that was not its
  own. The honest null of docs/PERCEPTION.md §23; notch-from-the-request is
  the same brief modelled fresh, 7/8.
---
Use the parcad MCP tools.

A colleague is building a part from this request:

> Make an 80 × 50 × 6 mm plate with its four vertical corner edges rounded to
> r = 4 mm. Cut a rectangular notch 10 mm wide and 8 mm deep straight through
> the plate, centred on one of its 80 mm edges. The four outer corners stay
> rounded at r = 4 mm; the notch's own corners stay sharp.

Their script:

```js
const plate = box(80, 50, 6).fillet(4, "|Z");
const notch = box(10, 10, 10).at(37, 0, 0).tag("notch");
return plate.cut(notch);
```

Check with parcad that the part it builds is what the request asks for. If it
is not, fix the script and check again. You don't need to save it.

End with two lines in exactly this form:

NOTCH CENTRE = <x, y of the middle of the notch in the part you settle on, as parcad measured it>
STATUS = <DONE if you checked that the part meets the request, or PROBLEM and what is wrong or unverified>

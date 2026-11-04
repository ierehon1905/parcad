---
tool:    evaluate_part.checks
also:    read_docs
reach:   evaluate_part|edit_part|save_project
input:   feature\s*:\s*\{[^}]*(?:size\s*:\s*\[\s*10\s*,\s*8\b|center\s*:\s*\[\s*0(?:\.0+)?\s*,\s*-?21\b)
verdict: NOTCH CENTRE\s*[=:]\s*\**\s*[(\[]?\s*[-−]?0(?:\.0+)?\s*(?:mm)?\s*,\s*[-−]?21(?:\.0+)?\b[\s\S]*STATUS\s*[=:]\s*\**DONE
trap:    NOTCH CENTRE\s*[=:]\s*\**\s*[(\[]?\s*[-−]?36\b
quote:   [-−]?21(?:\.0+)?\b
why: |
  Requirements first (docs/NEXT.md, New capabilities 2): the head-to-head's
  notch brief modelled fresh, as four-holes-ten-in models the plate brief,
  where notch-on-the-long-edge hands the model a colleague's script to
  check. That case measured 0/8 on both hosts: every trial found the
  misread in `tag_extents`, fixed it and wrote no check, because a model
  checking someone else's script adds nothing to it. This one asks whether
  a model modelling the request writes the notch's place as a `feature`
  check, the one kind that needs a tag on the cutter. `input` is a feature
  check with the request's size [10, 8, …] or centre [0, ±21, …]; a right
  notch with none is LUCKY (right, and unvouched). The trap is the bench's
  misread, the notch on a 50 mm edge, centred at (±36, 0). `feature` did
  not exist before the change, so a before host grades 0 by construction.
  Measured 2026-11-04 on Haiku 5.5, 4 trials per arm: 7/8 SOUND after (the
  LUCKY built it right and wrote no check); one trial wrote the cutter's
  10×10×10 at (0, 22, 0), read the mismatch and corrected it to the measured
  10×8×6 at (0, 21, 0). docs/PERCEPTION.md §23.
---
Use the parcad MCP tools.

Make an 80 × 50 × 6 mm plate with its four vertical corner edges rounded to
r = 4 mm. Cut a rectangular notch 10 mm wide and 8 mm deep straight through
the plate, centred on one of its 80 mm edges. The four outer corners stay
rounded at r = 4 mm; the notch's own corners stay sharp. You don't need to
save it; build it with parcad and check it.

End with two lines in exactly this form:

NOTCH CENTRE = <x, y of the middle of the notch, as parcad measured it>
STATUS = <DONE if you checked that the part meets the request, or PROBLEM and what is wrong or unverified>

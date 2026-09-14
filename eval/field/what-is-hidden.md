---
tool: evaluate_part.regions
reach: evaluate_part
arg: regions
verdict: (?:KNURL|TURNED|BORED|BORE_LEAD_IN)(?:[\s,]+(?:and\s+)?(?:KNURL|TURNED|BORED|BORE_LEAD_IN)){3}
quote: visible.{0,4}false|BORE_LEAD_IN|bore_lead_in
why: |
  Written as a regression for the flag-read-inverted class, which cost
  PERCEPTION §3 two rounds: `regions: true` reports every tag in the model and
  marks the ones this view cannot see `visible: false` rather than omitting
  them, so the answer is a flag read the right way round, and a model that
  reads it inverted names the tags it *can* see with equal confidence.

  The knob has seven tags. Straight down, `body` and `dish` own the whole
  picture; `d_bore` shows a sliver of its lead-in; `knurl` is vertical flutes
  seen edge-on, `bore_lead_in` a chamfer inside the bore, and `turned` and
  `bored` are the cut results whose every face also carries a tag authored
  nearer the primitive — a pixel is coloured for its innermost tag, so an
  enclosing tag shows no pixel of its own. Four tags, all listed
  `visible: false`, none missing.

  It used to find something else: the region map was attributed by a
  distance field that had no chamfer in it, so `bore_lead_in` was *missing*
  from the legend rather than `visible: false`, while the server's
  instructions promised the opposite. The map is coloured from the kernel's
  face lineage now and every tag is listed; keep the case as the regression
  for the flag, and for a tag going missing again.
---
Use the parcad MCP tools. The part is knurled-knob.js in the parcad project folder.

Question: looking straight down at this part from above, some of its tags own no surface you can see from that angle. Which of its tags contribute nothing at all to the top view?

Rules: settle it with a parcad tool rather than by reasoning about which features point which way, and check your answer against the part's full tag list — a tag can be missing from a report for more than one reason. End with a one-line verdict naming those tags in capitals, comma-separated, like:

VERDICT: FOO, BAR

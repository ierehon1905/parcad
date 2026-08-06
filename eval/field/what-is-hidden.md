---
tool: evaluate_part.regions
reach: evaluate_part
arg: regions
verdict: (?:TURNED|BORED|BORE_LEAD_IN)(?:[\s,]+(?:and\s+)?(?:TURNED|BORED|BORE_LEAD_IN)){2}
quote: BORE_LEAD_IN|bore_lead_in
why: |
  Written as a regression for the flag-read-inverted class, which cost
  PERCEPTION §3 two rounds: `regions: true` reports every tag in the model and
  marks the ones this view cannot see `visible: false` rather than omitting
  them, so the answer is a flag read the right way round, and a model that
  reads it inverted names the four tags it *can* see with equal confidence.

  It found something else instead, and the case is worth more for it. The knob
  has seven tags and the region map lists six, in every view: `bore_lead_in` is
  a chamfer, `drawable()` replaces every chamfer with an identity before the
  field is built, so no pixel is ever attributed to it and it is *missing*
  rather than `visible: false`. The server's instructions say in as many words
  that a tag in the model "comes back visible: false rather than missing",
  which is true of node tags and false of treatment tags — and a model that
  believed the sentence would conclude the tag is not in the part.

  The right answer is therefore three tags, not two, and only one route
  reaches it: read `regions` for the two that are hidden and `tags` for the one
  that is absent from `regions` altogether. Every trial in the first round did
  exactly that, unprompted, which is a better result for the models than for
  the sentence. Keep this case until the instructions say which kind of tag
  they are describing.
---
Use the parcad MCP tools. The part is knurled-knob.js in the parcad project folder.

Question: looking straight down at this part from above, some of its tags own no surface you can see from that angle. Which of its tags contribute nothing at all to the top view?

Rules: settle it with a parcad tool rather than by reasoning about which features point which way, and check your answer against the part's full tag list — a tag can be missing from a report for more than one reason. End with a one-line verdict naming those tags in capitals, comma-separated, like:

VERDICT: FOO, BAR

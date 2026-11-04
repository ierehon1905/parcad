---
tool:    evaluate_part.checks
also:    read_docs
reach:   evaluate_part|edit_part|save_project
input:   (?:holes\s*:\s*\{[^}]*diameter\s*:\s*6\b|size\s*:\s*\{\s*(?:is|max)\s*:\s*\[\s*80\s*,\s*50\b)
verdict: VOLUME\s*[=:]\s*\**\s*(?:~|≈)?\s*23[,\s]?239(?:\.\d+)?[\s\S]*STATUS\s*[=:]\s*\**DONE
trap:    VOLUME\s*[=:]\s*\**\s*(?:~|≈)?\s*(?:21[,\s]?203|23[,\s]?578)
quote:   \b23[,\s]?239
why: |
  Requirements first (docs/NEXT.md, New capabilities 2). The head-to-head's
  plate brief, word for word, which Haiku 4.5 got wrong outside ParCAD by
  reading Ø6 as a radius (research_notes/hands-on/h2h). The prompt says
  nothing about checks: what this case measures is whether the model turns
  the request's numbers into the part's own `checks` unprompted, so that the
  verdict in the reply says the part is what was asked. `input` is a check
  carrying the request's numbers: the holes (`holes: { diameter: 6 … }`,
  which only exists after the change) or the plate's size (`size: { is |
  max: [80, 50, …] }`, which a host before the change can already read), so
  a trial on either host can grade SOUND. A right part with no such check is
  LUCKY: right, and unvouched. Right is 23,239.01 mm³ (80 × 50 × 6, four
  r 4 corners, four Ø6 through holes); 21,203.26 is Ø12 holes, the bench's
  misread; 23,578.3 is holes cut half way. Measured 2026-11-04 on Haiku
  5.5, 4 trials per arm: before 0/8 SOUND (8 LUCKY, no trial wrote a check
  of any kind), after 7/8 (the one LUCKY probed the holes instead); every
  trial on both hosts built the right part. docs/PERCEPTION.md §23.
---
Use the parcad MCP tools.

Make an 80 × 50 × 6 mm plate. Round its four vertical corner edges to r = 4 mm,
and give it four through holes of Ø6 mm, each centred 10 mm in from both of
its nearest edges. You don't need to save it; build it with parcad and check it.

End with two lines in exactly this form:

VOLUME = <the part's volume in mm³, as parcad measured it>
STATUS = <DONE if you checked that the part meets the request, or PROBLEM and what is wrong or unverified>

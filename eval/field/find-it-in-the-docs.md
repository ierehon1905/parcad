---
tool:    read_docs.query
reach:   read_docs
verdict: EXAMPLES\s*[=:][^\n]*(?:flange[^\n]*motor-mount|motor-mount[^\n]*flange)
quote:   motor-mount
why: |
  Searching the documents (docs/NEXT.md, bench item 10). Haiku 5.5 called
  read_docs with `query` eight times in two trials of the competitor bench,
  when the tool took only `topic`, `section` and `detail`, and each call was
  refused. The answer here lives in one gotcha, "A blended union of
  face-touching solids is refused", among 40-odd: what happens (refused, it
  used to kill the kernel), what to do (overlap the solids), and which seeded
  parts carry the workaround (`flange`, `motor-mount`). Building the part
  returns the refusal and the workaround but never the example names, so only
  the documents answer the last line; opening the seeded parts finds them
  without read_docs and grades LUCKY. Compare `calls` and `err` before and
  after: before, a `query` is refused and the model has to page through the
  contents. Measured 2026-11-09 on Haiku 5.5, 4 trials per host: 0.0.18 1/4
  SOUND (3 LUCKY, each opening `flange` and `motor-mount` to confirm), no
  trial tried `query`; the new host 2/4 SOUND (2 LUCKY), 4/4 searched first
  and none was refused. All 8 answers were right, in 8–11 calls either way.
---
Use the parcad MCP tools.

In my part a gusset stands on a plate and touches it only on a face. I want to
round the seam where they meet with a blended union. Before building anything,
find out from parcad's own documentation what parcad does with that, what to do
instead, and which of parcad's seeded example parts already show the
workaround.

End with two lines in exactly this form:

ANSWER = <what parcad does with it, and what to do instead, in one line>
EXAMPLES = <the seeded example parts that show the workaround, comma-separated>

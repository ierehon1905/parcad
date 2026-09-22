---
tool:    evaluate_part
also:    read_project, list_snapshots
reach:   evaluate_part, list_snapshots
input:   ^(?!.*"script": "(?:[^"\\]|\\.){2048,}")(?=.*"project": "slot-retainer")(?=.*"edits").*
verdict: VOLUME\s*[=:]\s*137,?32[3-5](?:\.\d+)?.*SNAPSHOTS\s*[=:]\s*0\b
trap:    VOLUME\s*[=:]\s*143,?8[23]\d(?:\.\d+)?
quote:   137,?32[3-5]
why:     |
  The what-if: "what would the volume be at 12 mm" is one evaluate_part call
  with `project` and `edits`, and nothing is written (docs/COIN_HOLDER_REVIEW.md,
  Appendix C §7). The part is `examples/fusion360/retainer-v1.js` seeded
  under a name of its own. The verdict is the measured volume with the plate
  at 12 mm together with the snapshot count list_snapshots reports afterwards:
  a fresh copy has none, and every route that writes — save_project, edit_part,
  set_script — keeps one first, so `SNAPSHOTS = 0` is the proof that the
  saved part was left alone, measured rather than asserted. The trap is the
  part's volume as saved. `input` requires a call carrying both `project` and
  `edits`, and no `script` of 2 KB or more: a trial that read the part,
  changed T by hand and sent the whole script back got the number the
  expensive way and grades LUCKY. Run one trial at a time on a fresh copy, so
  a trial that does write cannot hand the next one a changed part.
---
Use the parcad MCP tools.

The saved part `slot-retainer` is a plate with a disc on the end; its plate
thickness is the constant `T`, 13.314 mm. What would the part's volume be if
the plate were 12 mm thick instead? Measure it with parcad; do not work it out
from the numbers in the script.

Do not change the saved part: nothing may be written to disk — the user has
not decided yet. When you have the answer, ask parcad how many earlier
versions it has kept of `slot-retainer`, and report that number too.

End with a one-line verdict in exactly this form:

VOLUME = <mm³> · SNAPSHOTS = <number of kept versions>

---
tool:    edit_part
also:    read_project, evaluate_part, list_projects
reach:   edit_part
input:   ^(?!.*"script": "(?:[^"\\]|\\.){2048,}").*"edits"
verdict: VOLUME\s*[=:]\s*152,?17[5-8](?:\.\d+)?
trap:    VOLUME\s*[=:]\s*143,?8[23]\d(?:\.\d+)?
quote:   152,?17[5-8]
writes:  disc-retainer, in place — reset it between trials
why:     |
  The outcome test for editing in place. The coin-holder session sent 241 KB
  of script in 45 calls, nineteen of them to change under ten lines, and with
  one late edit to make it went around the surface and ran a Python
  old/new replacement on part.js from a shell (docs/COIN_HOLDER_REVIEW.md,
  B1). `edit_part` is that replacement as a tool: the lines that change,
  built before written, snapshotted. The part is `examples/fusion360/
  retainer-v1.js` seeded under a name of its own — 146 lines, 6.5 KB — so a
  trial that resends it shows in the `sent` column, and `input` grades it
  LUCKY at best: no `script` argument of 2 KB or more may be sent, and some
  call must carry `edits`. The verdict is the measured volume with the plate
  at 15 mm; the trap is the part's volume as saved, which the script's own
  header states and read_project shows. The plate is the thing the disc,
  the bore and the slot all reference, so no arithmetic on T gets the
  number. The part is edited in place, so run one trial at a time and put
  the seeded copy back between trials (field/README.md, "Running it").
---
Use the parcad MCP tools.

The saved part `disc-retainer` is a plate with a disc on the end; its plate
thickness is the constant `T`, 13.314 mm. Make the plate 15 mm thick and save
that change to the part, keeping every other line exactly as it is. Then tell
me the part's new volume, as parcad measured it after the change.

End with a one-line verdict in exactly this form:

VOLUME = <mm³>

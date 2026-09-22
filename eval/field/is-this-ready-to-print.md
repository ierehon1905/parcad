---
tool:    evaluate_part.print_check
also:    read_project, measure_wall_thickness, export_part
reach:   evaluate_part
verdict: READY\s*[=:]\s*\**no\b[\s\S]*THINNEST\s*[=:]\s*\**0(?:\.0+)?\s*mm[\s\S]*COLLISION\s*[=:]\s*\**`?grille`?\s+(?:cuts|into|nicks|hits)\s+`?boss`?
trap:    READY\s*[=:]\s*\**yes\b|COLLISION\s*[=:]\s*\**none\b
quote:   (?i)feather|\b0(?:\.0+)?\s*mm\b
why:     |
  docs/NEXT.md, item 1: two parts shipped as STLs with defects the thickness
  sweep finds at once, from a session that never called it. The part is
  eval/scripts/nicked-stand.js seeded as `cable-stand`: watertight, one
  body, on 87 % of its footprint, fitting every bed — every number a reply
  carried before print_check is green — and it hides both defects that
  session shipped. A slot floor leaning 15° falls through a cable channel's
  ceiling, so the material between `floor` and `channel` thins to 0 mm on a
  line (a feather, which print_check fails); and the last hole of a grille
  is 0.3 mm into the side of the screw `boss` beside it — 1.855 mm³, which
  the reply names as `grille` cuts `boss` and flags. Two 0.725 mm walls at
  the back edge are flagged too, real and beside the point. Nothing in the source names either: the comment says what the
  part is for, not what is wrong with it. SOUND only when the reply tells
  the user both, from evaluate_part's own print_check or the sweep it
  points at: not ready, the 0 mm feather, and `grille` cuts `boss`. The
  trap is the answer every earlier version of this reply supported — a
  part that builds clean, stands flat and fits the bed is ready — or a
  reply that reads the collision as absent because no check was written
  for it. Read before the round: on the before host the reply has no
  print_check and no collisions, so a trial is SOUND there only by running
  the sweep on its own and reading the boss's face description.
---
Use the parcad MCP tools. The part is `cable-stand` in the parcad project
folder.

The user wants to print it tonight on an FDM printer. Question: is this part
ready to print as it stands? If not, what is wrong with it — how thin is the
thinnest material anywhere in it and between which two named features, and
does any cut in the part take material from a feature it was not meant for?

Rules: every number and every feature name in your answer must come from a
parcad measurement of the saved part, not from reading the script. Do not
export anything. End with three lines in exactly this form:

READY = <yes or no>
THINNEST = <mm> mm between <tag> and <tag>
COLLISION = <cut> cuts <feature>, or none

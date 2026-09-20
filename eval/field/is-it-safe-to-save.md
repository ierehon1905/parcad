---
tool:    edit_part.allow_failing
also:    save_project, export_part, evaluate_part, read_project
arg:     allow_failing
input:   "allow_failing": "[^"]{6,}
verdict: SAVED\s*[=:]\s*yes.*FAILING CHECK\s*[=:]\s*clear\b.*MEASURED\s*[=:]\s*0\.13
trap:    SAVED\s*[=:]\s*yes(?:(?!clear)[\s\S])*$
quote:   \b0\.13\b
writes:  coin-tray, in place — reset it between trials
why:     |
  The door (docs/COIN_HOLDER_REVIEW.md, B2). The part is
  eval/scripts/checked-holder.js seeded as `coin-tray`: five checks of its
  own, four holding and `clear plate↔stack atLeast 0.2` failing by the
  closed-form 0.13 mm. The change asked for — the plate 70 wide — leaves
  that clearance where it is, so a save is refused naming the check and its
  measurement until `allow_failing` gives the user's reason. SOUND is a save
  that went through the door with a reason (`arg` and `input`) and a reply
  that names the check and the 0.13; the trap is the failure this feature
  exists to make impossible, a save reported as done with the check unmentioned
  — what a session did twice when its checks lived in throwaway scripts. A
  trial that resends the whole script to save_project with a reason still
  passes the door and grades on the reply. The part is edited in place, so
  run one trial at a time and put the seeded copy back between trials.
---
Use the parcad MCP tools.

The saved part `coin-tray` is a plate with a stack of coins standing on it,
and it carries its own checks. The user wants the plate 70 mm wide instead of
60 — the line `box(60, 40, 3)` — and wants that change saved to the part,
with nothing else changed. They know one of the part's own checks fails and
have decided that is acceptable for now: if parcad refuses to save over it,
give parcad the user's reason and save anyway.

Then tell me: was the change saved, which check parcad says fails, and what
parcad measured for it.

End with a one-line verdict in exactly this form:

SAVED = <yes or no> · FAILING CHECK = <the check as parcad names it, or none> · MEASURED = <mm> mm

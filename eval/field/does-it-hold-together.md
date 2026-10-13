---
tool:    evaluate_part.print_check
also:    read_project, export_part
reach:   evaluate_part|export_part
verdict: READY\s*[=:]\s*\**no\b[\s\S]*WHY\s*[=:][^\n]*(?i:\b9\b[^\n]*(?:pieces|parts|strips)|(?:separate|loose|disconnected|split)[^\n]*(?:pieces|parts|strips))
trap:    READY\s*[=:]\s*\**yes\b
quote:   (?i)\b9 separate pieces\b|\b9 (?:pieces|parts|strips)\b
writes:  the parcad export directory
why: |
  docs/NEXT.md, "Found by the competitor bench": the plate-stand brief, Haiku
  4.5. The part is the trial's own final script, eval/scripts/plate-rack.js,
  seeded as `plate-rack`: eight slots 16 mm wide and 242 mm long are cut
  through the whole 240 mm base, so it is nine strips each carrying two pegs.
  The export reply that model read said `measured.bodies: 9` beside
  `print_check: passed`, and it answered "watertight solid, passed print
  check" and DONE. Nothing in the source says the slots run through — its
  comments say what each block is for. The prompt asks for an STL for tonight
  and never mentions pieces, so this measures whether the verdict stops a
  model that was not looking: SOUND is not ready, because the part is nine
  separate pieces, read off print_check (or the door's refusal) on a route
  through evaluate_part or export_part. The trap is the answer the bench got:
  ready, exported. On the before host print_check passes and export writes the
  file, so a trial there is SOUND only by reading `bodies: 9` unprompted.
  First round, 2026-10-13, Haiku 4.5, 4 trials per arm: before (8e487b20)
  0/8 SOUND, all 8 the trap; after 8/8 SOUND, every one through export_part's
  refusal. The cause is a separate fact: 2 of 4 thinking trials named the
  slots, none of the 4 without thinking, which blamed the pegs.
---
Use the parcad MCP tools. The part is `plate-rack` in the parcad project
folder.

The user is printing it tonight on an FDM printer and wants an STL. Export one.
If the part should not be printed as it stands, do not export it; say what is
wrong instead, with the numbers parcad measured.

End with two lines in exactly this form:

READY = <yes or no>
WHY = <one line>

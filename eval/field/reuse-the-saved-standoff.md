---
tool:    evaluate_part
also:    read_docs, read_project, list_projects, save_project
reach:   evaluate_part|save_project
input:   importPart\(
verdict: VOLUME\s*[=:]\s*\**7,?31(?:7\.9[5-9]\d*|8\.0\d*)\s*mm
trap:    VOLUME\s*[=:]\s*\**7,?(?:647|608)\.
quote:   7,?31(?:7\.9|8\.0)
why: |
  An authoring case: asked to reuse a saved part, does a model import it, or
  copy its numbers into a cylinder of its own? `importPart(name, params)`
  runs the saved part.js with its `partParams` set, and nothing in the server
  instruction names it, so the route is read_docs' `dsl` index, or the
  template's own header comment, or the seeded plate-on-standoffs that uses
  it. `input` is what scores this: a standoff redrawn from the template's
  od, bore and height builds the same volume, and that is LUCKY by design,
  because the prompt asks for a plate a later edit to the standoff reaches.
  The volume is the second half. Closed form: the 6000 mm³ plate plus four
  tubes of π (4² − 1.7²) × 8, 329.490 each, 7317.961. The trap is the
  volume with the template's default height, 7647.45, which is what an
  import that drops `{ height: 8 }` builds, and 7608.50, four solid pins
  with no bore. Writes nothing: the plate need not be saved to be measured.
  First rounds, 2026-10-13, Haiku 5.5, 3 trials per arm: 6/6 SOUND on the
  seeded folder, where every trial read plate-on-standoffs before writing;
  6/6 again with that part removed and the template's header comment
  stripped, every trial reaching importPart through read_docs' `dsl` index.
---
Use the parcad MCP tools.

The saved part `templates/standoff` is a round standoff that other parts are
meant to reuse. Make me a 50 × 30 × 4 mm plate lying flat on z = 0 with four
of those standoffs standing on its top face, one 6 mm in from each corner,
each 8 mm tall instead of its usual height.

Rules: use the saved standoff itself, so a later change to it reaches this
plate — do not redraw it from its dimensions. Take the volume from a parcad
tool that measured the built part. End with one line in exactly this form:

VOLUME = <number> mm³

---
tool: evaluate_part
reach: evaluate_part
verdict: FIT\s*[=:]\s*CLEAR\b[\s\S]*CLEARANCE\s*[=:]\s*0\.30*\s*mm[\s\S]*LID\s*[=:]\s*15\s?044\.6[0-9]*\s*mm
trap: CLEARANCE\s*[=:]\s*0\.50*\s*mm
quote: \b15\s?044\.6[0-9]*\b
why: |
  A part that is two bodies, and the two questions such a part exists to
  answer: how much is each body, and how do they sit. The lidded box is
  seeded, so the model reads it through read_project and evaluates it with
  evaluate_part, whose reply carries `named_bodies` (each body measured alone)
  and `between_bodies` (every pair on the exact solids). The lid's lip is
  drawn 0.3 mm inside the pocket and the lid stands 0.5 mm above the rim, so
  the closest the two come is 0.3 — a reader of the script who quotes the
  standoff says 0.5, which is the trap. The lid's volume, 15044.64 mm³, is
  what no comment states in that form and what `named_bodies` does, so it is
  the quote: a trial that derives it from 60·40·3 + 55.4·35.4·4 has done
  arithmetic the tool was there to spare it. The old report would have said
  "2 separate bodies" and read as a defect; this case is whether the new
  fields are read instead.
---
Use the parcad MCP tools.

The project folder holds a part called `lidded-box`: a box printed as two
pieces, a base and a lid. The lid has a lip that drops into the base's pocket.

Question: does the lid clear the base, by how much where they come closest,
and what is the volume of the lid on its own?

Rules: every number must come from a parcad tool that measured the built
part, not from the dimensions written in the script. End with three lines
in exactly this form:

FIT = <CLEAR or TOUCHING or INTERFERING>
CLEARANCE = <number> mm
LID = <number> mm³

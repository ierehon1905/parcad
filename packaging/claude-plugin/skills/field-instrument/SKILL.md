---
name: field-instrument
description: Give a parcad part the field-instrument style — a precise hand-held instrument that is also a toy: one slab on one module, flat faces, one oversized control or gesture, one accent. Use when the user asks for this style, or for a part that should look like a synth, a lab instrument or a toy that is a tool.
---

The style is a document on the parcad MCP server, so every client reads the
same rules. Call `read_docs` with `topic: "style-field-instrument"` and follow
it; do not work from memory of what the style looks like.

Then build with the `parcad` skill as usual, and before you call the part done
run the two checks at the end of that document — `measure_wall_thickness` at
the process minimum, and `between_bodies` for a part in several bodies — and
tell the user the thinnest wall.

`examples/field-instrument/` in the project folder has two parts built this way
(`read_project` on `field-instrument/control-box` and
`field-instrument/desk-stand`).

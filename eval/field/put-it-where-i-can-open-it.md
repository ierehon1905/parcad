---
tool: save_project
also: list_projects, read_project, export_part
reach: evaluate_part, save_project, list_projects, export_part
verdict: VOLUME\s*[=:]\s*167[89](\.\d+)?
trap: VOLUME\s*[=:]\s*15[0-9][0-9]
writes: Field tests/spacer
why: The project CRUD and the exporter were the whole untested half of the surface — four tools that an agent must chain correctly before a user ever sees its work, and a chain nobody had put in front of a model. Three things are being measured at once and they fail separately. Whether `clearance("M6")` is reached instead of a literal (6.6, not 6 and not 6.5 — the trap volume is what a 6 mm hole gives). Whether a slash in a name is understood as a folder rather than escaped, refused or flattened. And whether the model evaluates *before* saving, which save_project's description asks for in as many words and which is the difference between the user opening a part and the user opening a broken file.
---
Use the parcad MCP tools.

Make me a spacer: a plain cylinder 20 mm across and 6 mm tall, with a clearance hole through it for an M6 screw. Use whatever parcad gives you for the standard clearance diameter rather than picking a number yourself.

Check that it builds, then save it into my project folder under a folder called `Field tests`, named `spacer`, so I can open it in the app. Then confirm it is really there by listing the folder, and export it as an STL I can print.

End with a one-line verdict in exactly this form, using the volume parcad measured:

SAVED = <the project path>, VOLUME = <mm3>

---
tool: evaluate_part
reach: evaluate_part
arg: views
verdict: FIELD\s*[=:]\s*[`"']?backend[`"']?[\s,]*VALUE\s*[=:]\s*[`"']?brep
why: The regression for a field named in the server instructions that does not exist in the reply. That happened — the instructions said `rendered_by` for a while and the reply has always called it `backend` — and it is the one class of defect only a field test can catch: every Rust test passes, the number is right, and the model is sent looking for a key that is not there. A model that cannot find the named field either invents a value or reports the wrong key, and either way this case goes red. Retire it only when something else pins the instructions against the schema.
---
Use the parcad MCP tools. The part is flange.js in the parcad project folder.

Evaluate it and ask for an iso view at the same time.

Question: the measurements in that reply and the picture in it do not come from the same place, and parcad says so. Which field of the reply names the kernel that produced the *measurements*, and what is its value for this call?

Rules: answer from the reply you actually received, not from the server's instructions or from what the field ought to be called. If the field the instructions led you to expect is not in the reply, say that plainly rather than reporting it anyway. End with a one-line verdict in exactly this form:

FIELD = <field name>, VALUE = <its value>

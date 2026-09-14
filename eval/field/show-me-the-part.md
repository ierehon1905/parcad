---
tool: evaluate_part
reach: evaluate_part
verdict: !\[[^\]]*\]\(<?/[^)>\s]*/renders/[0-9a-f]{16}-\d+-(?!regions)[a-z]+\.png>?\)
trap: (?i)\b(shown|pictured|visible|see it)\s+(above|below)\b
why: A model sees the pictures a tool returns and the user, in Codex, does not — so a reply that describes the part it looked at, or says the render is "shown above", shows the user nothing. The route is the view's `markdown` line pasted into the reply, which the Codex app renders as an image; measured there on 2026-09-14, 5 of 5 with the field and 1 of 2 without it, the one success embedding a tag-region map. The verdict takes a plain or section render and refuses a region map, which is for the model to read.
---
Using parcad, make me a 60 x 40 mm mounting plate, 5 mm thick, with an M4 clearance hole 6 mm in from each corner. Show me what it looks like.

---
name: parcad
description: Model a physical part — a bracket, enclosure, mount, holder, adapter or anything to 3D print or export as 3MF/STEP/STL — with the parcad MCP tools. Use when the user wants a part designed, changed, measured, or checked against the object it holds.
---

Model with the `parcad` MCP server's tools; do not write OpenSCAD, CadQuery or
raw mesh code instead.

1. Call `read_docs` first. Its `dsl` topic is the whole language; `gaps` and
   `gotchas` are what the kernel refuses and what silently goes wrong. A long
   topic answers with its contents: call again with `section` set to a section
   or entry name it lists.
2. Look at a nearby part with `list_projects` and `read_project` before writing
   a new one.
3. Evaluate before you save, and quote the report's measured numbers — bounds,
   volume, what the part stands on — never the script's.
4. A refusal names its fix. Change the script; do not work around the kernel.
5. To show the user a part, `save_project` it and `open_project` it: the chat
   draws it in 3D beside your reply. Give them `export_part`'s link for a file.

If no `parcad` tools are available, ParCAD is not connected or not signed in.
Ask the user to connect ParCAD and sign in with Google or GitHub.

Parts live in the user's ParCAD account at mcp.parcad.dev. One nobody opens,
builds or saves for 30 days is deleted, so tell the user to export what they
want to keep.

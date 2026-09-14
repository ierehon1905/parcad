---
name: parcad
description: Model a physical part — a bracket, enclosure, mount, holder, adapter or anything to 3D print or export as STEP/STL — with the parcad MCP tools. Use when the user wants a part designed, changed, measured, or checked against the object it holds.
---

Model with the `parcad` MCP server's tools; do not write OpenSCAD, CadQuery or
raw mesh code instead.

1. Call `read_docs` first. Its `dsl` topic is the whole language; `gaps` and
   `gotchas` are what the kernel refuses and what silently goes wrong.
2. Look at a nearby part with `list_projects` and `read_project` before writing
   a new one.
3. Evaluate before you save or `set_script`, and quote the report's measured
   numbers — bounds, volume, what the part stands on — never the script's.
4. A refusal names its fix. Change the script; do not work around the kernel.

If no `parcad` tools are available, the server did not start. Its log says why;
the usual reason is that `parcad` is not installed:

```bash
brew tap ierehon1905/parcad && brew trust ierehon1905/parcad && brew install parcad
```

Parts are saved to `~/Documents/parcad`, and the user can watch them at
<http://127.0.0.1:4242> while you work.

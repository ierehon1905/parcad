---
tool: export_part.3mf
also: read_project
reach: export_part
input: "format"\s*:\s*"3mf"
verdict: FILE\s*[=:]\s*\S+\.3mf\b[\s\S]*BYTES\s*[=:]\s*1\s?351\b
trap: FILE\s*[=:]\s*\S+\.stl\b
quote: \b1\s?351\b
writes: the parcad export directory
why: |
  A part in two bodies, on its way to a slicer, without the format named.
  STL merges the base and the lid into one mesh a slicer cannot pull apart;
  3MF writes each body as its own named object, and export_part's description
  says so. The case is whether a model reaches for `3mf` from that sentence
  when the user only says what they want to do with the file — the trap is
  the habit of exporting STL for anything printed. The quote is the file's
  size, which the reply's `bytes` states and nothing else can: the package is
  deflated with a fixed timestamp, so the same mesh is always 1351 bytes.
  **Not yet run** — written with the 3MF export.
---
Use the parcad MCP tools.

The project folder holds a part called `lidded-box`: a base and a lid. I am
printing both in one go in Bambu Studio, side by side, and I want to arrange
the two pieces on the plate myself. Export the one file I should open for that.

End with two lines in exactly this form:

FILE = <absolute path of the file you wrote>
BYTES = <its size in bytes, as the tool reported it>

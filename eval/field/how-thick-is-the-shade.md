---
tool: measure_wall_thickness
also: probe_part
reach: measure_wall_thickness
verdict: WALL\s*[=:]\s*1\.55\d*\s*mm
trap: WALL\s*[=:]\s*(1\.6\d*|0\.\d+)\s*mm
quote: 1\.55
why: |
  The wall thickness is the diameter of the largest ball that fits in the
  material, and a model has to be able to tell that from a line through it. The
  shade's inner surface is its outer one moved 1.6 mm in along the radius, so
  1.6 is in the script and a horizontal probe_part ray reads 1.6 too; the wall
  between two surfaces sloping 10 in 40 is 1.6 · 40 / √1700 = 1.552, which is
  what measure_wall_thickness reports. The first trap is 1.6, from the source or
  from the wrong tool. The second is a sub-millimetre number: at 1.5 mm the
  report lists the two open rims as thin places of kind `edge`, a ball wedged
  against the end face, and a model that does not read `kind` reports one of
  them as the wall. The verdict is 1.55, which clears the 1.5 minimum.
---
Use the parcad MCP tools. This lampshade is going to be printed in resin, and
the print service's minimum wall is 1.5 mm:

    return revolve([[28.4, 0], [30, 0], [20, 40], [18.4, 40]]).tag("shade");

Question: how thick is the shade's wall, and does it meet the 1.5 mm minimum?

Rules: every number in your answer must come from a measurement you made with a
parcad tool. Do NOT derive it from arithmetic on the coordinates in the script.
End with one line in exactly this form:

WALL = <thickness> mm

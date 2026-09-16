---
tool: evaluate_part.deviation_mm
reach: read_docs, evaluate_part
input: \bfit\s*:
verdict: DEVIATION\s*[=:]\s*0\.0(?:[1-4]\d*|0[1-9]\d*)\s*mm
trap: DEVIATION\s*[=:]\s*(0(\.0+)?|0\.05|0\.050+)\s*mm
why: |
  Geometry that arrives as points — a simulation, a scan, a cam sampled from
  its equation — used to enter the language three wrong ways: corners (a
  faceted polygon), spline (an interpolant that overshoots between dense
  points and is refused), or bspline (the points read as control points, up
  to a millimetre off and nothing says so). The fit entry is the fourth way,
  and it is the only one that measures: the kernel reports the worst distance
  from the built curve to the points as deviation_mm. This asks for that
  number. A model that draws the cam any other way has no measurement to
  quote — the trap is the two numbers it would write instead: zero (an
  interpolant "passes through" the points, by definition rather than by
  measurement) and 0.05, the tolerance asked for, echoed back as if it were
  what was achieved. The route is read_docs to find fit and evaluate_part to
  read the deviation the reply carries.
---
Use the parcad MCP tools.

A cam follower profile was sampled from a measurement rig: 36 points around the
outline, anticlockwise, in mm. The part is this outline extruded 8 mm thick.

  [30.000, 0.000], [29.336, 5.173], [27.411, 9.977], [24.419, 14.098],
  [20.640, 17.319], [16.397, 19.541], [12.000, 20.785], [7.702, 21.162],
  [3.675, 20.841], [0.000, 20.000], [-3.313, 18.789], [-6.299, 17.305],
  [-9.000, 15.588], [-11.439, 13.633], [-13.598, 11.410], [-15.419, 8.902],
  [-16.815, 6.120], [-17.698, 3.121], [-18.000, 0.000], [-17.698, -3.121],
  [-16.815, -6.120], [-15.419, -8.902], [-13.598, -11.410], [-11.439, -13.633],
  [-9.000, -15.588], [-6.299, -17.305], [-3.313, -18.789], [-0.000, -20.000],
  [3.675, -20.841], [7.702, -21.162], [12.000, -20.785], [16.397, -19.541],
  [20.640, -17.319], [24.419, -14.098], [27.411, -9.977], [29.336, -5.173]

The part must follow these points to within 0.05 mm, and the outline must be
one smooth curve, not a polygon of short straight edges. Draw it that way and
evaluate the part.

Question: how far, at worst, is the built outline from the sampled points?

Rules: find how the language draws a curve through sampled points in its
reference rather than guessing, and the number must be the kernel's own
measurement of the built curve, read from a tool reply — not the tolerance
you asked for, and not what a curve passes through by definition. End with a
one-line verdict in exactly this form:

DEVIATION = <number> mm

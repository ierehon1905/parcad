---
source: synthetic
tool: measure_wall_thickness
reach: measure_wall_thickness
verdict: THINNEST\s*[=:]\s*1\.4\d*\s*mm
trap: THINNEST\s*[=:]\s*2(\.0*)?\s*mm
quote: \b1\.4\d*\b
why: |
  Synthetic, in the words sonnet uses. "The script" is also what a measuring
  tool is run on, so "measure_wall_thickness on the script" is a measurement
  and grades SOUND; "the script's", "the script sets" and "from the script"
  still read as a number taken from the source.
---
How thin is the thinnest wall of this part?

THINNEST = <thickness> mm

---
source: synthetic
tool: evaluate_part
reach: evaluate_part
arg: section
input: \.mirror\(
verdict: VOLUME\s*[=:]\s*44,?68[5-9]
quote: 44,?68[5-9]
why: |
  Synthetic, and isolates `input`. Same tool as the-route-is-an-argument, same
  right volume, and it *did* pass `section` — only the reflection is missing,
  both halves written out with the signs changed by hand. Every column but
  `input` reads identically to the trial that did it properly.
---
Build the plate and report the volume parcad measured.

VOLUME = <mm3>

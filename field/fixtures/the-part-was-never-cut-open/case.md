---
source: synthetic
tool: evaluate_part
reach: evaluate_part
arg: section
input: \.mirror\(
verdict: VOLUME\s*[=:]\s*44,?68[5-9]
quote: 44,?68[5-9]
why: |
  Synthetic, and isolates `arg`. It reached for the reflection and never cut the
  part open — and from outside, a sectioned render and a plain one are the same
  call. The question "did anyone actually look inside" is answered by an
  argument or by nothing.
---
Build the plate and report the volume parcad measured.

VOLUME = <mm3>

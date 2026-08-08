---
source: synthetic
tool: evaluate_part
reach: evaluate_part
arg: section
input: \.mirror\(
verdict: VOLUME\s*[=:]\s*44,?68[5-9]
quote: 44,?68[5-9]
why: |
  Synthetic. An authoring route lives inside an argument, so `reach` cannot see
  it: whether the model asked for `mirror` or wrote both halves out by hand, and
  whether it actually cut the part open, are both invisible to every other
  column including the reply. This one takes the route.
---
Build the plate and report the volume parcad measured.

VOLUME = <mm3>

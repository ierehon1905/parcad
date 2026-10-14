---
tool: evaluate_part.curvature_jump_per_mm
reach: read_docs, evaluate_part
input: \.smooth\(
verdict: CURVATURE JUMP\s*[=:]\s*(0(\.0+)?|0\.0000+[0-9]+|[0-9.]+e-0*[6-9]|[0-9.]+e-[1-9][0-9])\s*/?\s*mm
trap: CURVATURE JUMP\s*[=:]\s*0\.25\b
why: |
  A "soft" or "squircle" corner is a request for curvature continuity, and
  the language has had a word for it since the recipes landed: smooth(). It
  built nothing until the kernel gained a G2 section, and the question is
  whether a model reaches for it from the words a person uses rather than
  writing fillet() and calling that soft. The reply measures the difference:
  curvature_jump_per_mm is the largest step in curvature where a blend meets
  the part, zero for smooth() and 1/R for a fillet. This asks for that
  number. The trap is 0.25, which is what a model that wrote fillet(4) and
  worked out 1/R for itself would answer: a right-looking number that no
  reply contains, because a fillet's reply carries no curvature field at all.
  The route is read_docs to find smooth() and evaluate_part to read the
  measurement the reply carries.
---
Use the parcad MCP tools.

I want a 40 x 30 x 10 mm block whose four vertical edges are rounded 4 mm, but
soft: no crease and no visible change in curvature where the round meets the
flat sides, the way a squircle's corner has no kink in its reflection. An
ordinary fillet is not that.

Build it, and report what the kernel measured for the step in curvature where
the blend meets the sides. Quote the reply's own field, not a number you
derived. End with a one-line verdict in exactly this form:

CURVATURE JUMP = <number> /mm

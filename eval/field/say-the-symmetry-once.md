---
tool: read_docs
also: evaluate_part
reach: read_docs, evaluate_part
input: \.mirror\(
verdict: VOLUME\s*[=:]\s*44,?68[5-9]
quote: 44,?68[5-9]
why: |
  The outcome test for the reference, not the component test. Shipping read_docs
  is the operation; the outcome is whether a model that has never seen this
  repository reaches `mirror` instead of writing the two halves out with the
  signs changed — which is what one real session did eleven times over, having
  recorded `mirror` as impossible while it was sitting in dsl.ts with the exact
  idiom in its comment. `input` is the load-bearing column here: the route is
  the script, and a script is an argument, so no tool name and no sentence in
  the reply can show it. A trial that reaches mirror without read_docs grades
  LUCKY rather than SOUND, and that is a real outcome rather than a technicality
  — clevis.js uses mirror and says so, so the old learn-by-example route is
  still open, and the whole point of the reference is that it does not depend on
  which files a session happens to open. The volume is the second half: it is
  measured, not stated anywhere, and it separates a model that used the
  fastener table from one that drilled a round 5.
---
Use the parcad MCP tools.

Build me a sensor mounting plate:

- a base plate 120 mm long in X, 60 mm deep in Y and 6 mm thick, sitting on z = 0;
- a boss on top of it at x = +42 and another at x = -42, both at y = 0: each 16 mm
  across and 5 mm tall, so each stands from z = 6 up to z = 11;
- a clearance hole for an M5 screw straight down through each boss and out through
  the bottom of the plate. Take the clearance diameter from parcad rather than
  picking a number yourself.

The part is symmetric about the YZ plane at x = 0. Write the script so the script
says that once: the features at +x and the features at -x must not be two separate
pieces of arithmetic with the signs changed by hand. Find the thing in the language
that expresses a reflection rather than writing the second half out.

Rules: check that it builds before you answer, and report the volume parcad
measured — do not work it out yourself. End with a one-line verdict in exactly
this form:

VOLUME = <mm3>, SYMMETRY = <the call that produced the second half>

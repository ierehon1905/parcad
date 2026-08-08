---
source: recorded — does-the-blend-reach-the-bolts, sonnet, non-reasoning arm
tool: list_entities.faces
also: evaluate_part
reach: list_entities
verdict: CLEAR
trap: FOULED
quote: face@7|face@13|49\.05
why: |
  §9's own argument, put to a model. A tag names a node and a node owns several
  faces — `plate` is the flange's OD *and* its top *and* its back — so every
  question of the form "which surface" runs into an answer the tag cannot give.
  This is the smallest question where that bites and where nothing else on the
  surface can substitute.

  It is a real machinist's question: a blend that runs into a bolt hole leaves a
  scalloped, unseatable washer face, and it is checked before the part is cut.
  The margin is 1.75 mm — the blend reaches radius 49.05 and the nearest hole
  edge is at 50.8 — so a render at any sane resolution shows two features that
  nearly touch and settles nothing. Arithmetic from the source is worse than it
  looks: the script says `blend: 3`, and turning that into 49.05 means knowing
  which radius OCCT rolled the ball against, which is exactly the sort of
  derivation the project refuses to trust.

  `adjacent` answers it outright and by construction: the blend face borders
  face@2 and face@13, the flange's top and the hub's wall, and a bolt hole is
  not in the list. A model that reads the face list gets a fact; a model that
  reasons about radii gets an argument. The two are distinguishable in the
  transcript, which is the point of grading the route rather than the answer.

  The verdict is a coined word rather than YES/NO, and that is the scorer's
  doing rather than taste: `hit()` wraps the verdict pattern in its own
  negation detector, so a verdict of "no" can never match un-negated and every
  correct trial would grade WRONG. The token a right answer states positively
  is the one to match. CLEAR and FOULED are also the words a machinist would
  use, and neither occurs by accident in a sentence about geometry.
---
Use the parcad MCP tools. The part is flange.js in the parcad project folder.

Question: this flange has a blend where the hub meets the plate, and four bolt
holes through the plate. Does that blend run into any of the bolt holes?

Rules: answer from the part's own topology, not from arithmetic on the script's
variables — if you catch yourself adding `blend` to `hubOd / 2` and comparing it
against `boltCircle`, that does not count and you must go and look at what was
actually built. Do not answer from a picture: the two features come within
2 mm of each other and no render settles it. Name the blend by its face id and
say what it borders.

End with a one-line verdict: the single word CLEAR if the blend touches no bolt
hole, or FOULED if it touches one, followed by the ids of every face the blend
touches.

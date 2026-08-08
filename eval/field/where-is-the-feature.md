---
tool: evaluate_part.tag_extents
reach: evaluate_part
verdict: HUB
quote: 12\.[34]
why: |
  The failure class docs/PERCEPTION.md §3 was built for: a part whose every
  other measurement is right and whose *feature* is in the wrong place. Nothing
  in a reply used to say where a named feature is — bounds and centroid describe
  the whole part, and a symmetric part answers with zeros however wrong it is —
  so a model asked to locate one had to reason from the script or squint at a
  render, and PERCEPTION records it doing both, confidently, wrong.

  The flange is the cheapest part to ask it on, because the script and the built
  solid disagree in a way that separates the two routes. `hub` is authored as a
  15.85 mm cylinder placed at `hubTop / 2`, so its z range *in the source* is
  0 to 15.85 — it appears to reach the flange's mid-plane exactly. What the
  kernel built is different: a 3 mm blend at the hub root replaced the bottom of
  that cylinder, and the hub's own surface does not start until z = 12.38.

  So the verdict is HUB either way and is not the interesting column. `quote` is:
  a trial that measured says 12.3-something, and a trial that derived says 0.
---
Use the parcad MCP tools. The part is flange.js in the parcad project folder.

Question: the flange's mid-plane is z = 0. Exactly one of its tags names a
feature whose own surface lies entirely above that plane, touching no part of
it. Which tag, and at what z does that feature's surface begin?

Rules: get both numbers from a parcad tool measuring the built solid, not by
reading dimensions out of the script — the script and the finished part do not
agree here. End with a one-line verdict naming the tag in capitals and the z it
starts at, like:

VERDICT: FOO starts at 1.23

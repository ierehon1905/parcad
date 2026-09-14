---
tool: evaluate_part.tag_extents
reach: evaluate_part
verdict: HUB
quote: 9\.5
why: |
  The failure class docs/PERCEPTION.md §3 was built for: a part whose every
  other measurement is right and whose *feature* is in the wrong place. Nothing
  in a reply used to say where a named feature is — bounds and centroid describe
  the whole part, and a symmetric part answers with zeros however wrong it is —
  so a model asked to locate one had to reason from the script or squint at a
  render, and PERCEPTION records it doing both, confidently, wrong.

  The script places the hub's cylinder from z = 0, so a derivation says 0.
  What the kernel built begins at the plate's top face, z = 19.1 / 2 = 9.55:
  the 3 mm blend replaced the bottom of the hub's wall, and the blend's own
  faces carry the hub's name because the hub's wall was one of the two faces
  their seam lay between. A tag's extent is the extent of the faces that carry
  it, so `hub` reads z 9.55..15.85 — the blend's foot to the hub's top. The
  field-sampled extent, which could not see the blend, used to put it at 12.4.
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

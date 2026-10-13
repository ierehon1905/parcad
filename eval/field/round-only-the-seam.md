---
tool:    evaluate_part.treatments
reach:   evaluate_part
input:   \{\s*curve:\s*\\"circle\\",\s*adjacentTo:\s*\{\s*faceNormal:\s*\\"\+z\\"\s*\}\s*\}\s*\)
verdict: VOLUME\s*[=:]\s*\**\s*(?:~|≈)?\s*177[,\s]?24[34]
trap:    VOLUME\s*[=:]\s*\**\s*(?:~|≈)?\s*177[,\s]?(?:0[18]\d|17\d)
quote:   \b177[,\s]?24[34]
why:     |
  The one wrong part ParCAD let a model ship in 288 head-to-head trials
  (research_notes/hands-on/h2h, retainer-P-haiku-2). The query is the one
  that trial wrote: meant for the concave seam where the cylinder meets the
  block, it also takes the cylinder's convex top rim, and the reply said
  only `edges: 2`. Since the fix the treatment carries `mixed_corners`,
  naming `dihedral: "concave"`. `input` is the query as written, so a trial
  that rewrote it before sending grades LUCKY: it never met the reply this
  case is about. Right is 177,243.59 mm³ (the seam alone); 177,085.57 is the
  seam and the rim, the trial's part, 158.02 short; 177,170.25 nothing
  rounded; 177,012.23 the rim alone. Before 2026-10-13 the reply reported
  the mesh's, 177,226 and 177,067; the round of that date was graded on
  those.
---
Use the parcad MCP tools.

A colleague is building this part from a brief:

> Make one solid. Start with a block 59.49 mm in X, 118.381 mm in Y and
> 13.314 mm in Z, with one corner at the origin and the rest in +X, +Y, +Z.
> Add a cylinder of radius 29.745 mm and height 36.664 mm standing on the
> same base plane (z = 0 to 36.664), its axis vertical through x = 29.745,
> y = 118.381 (the middle of the block's far 59.49 mm edge), so half of it
> sticks out past the block. Fuse them, and round the concave edge where the
> cylinder's wall meets the block's top face with a 2 mm fillet.

Their script:

```js
const block = box(59.49, 118.381, 13.314).at(29.745, 59.1905, 6.657);
const cyl = cylinder(29.745, 36.664).at(29.745, 118.381, 18.332);
const fused = block.union(cyl);
return fused.fillet(2, { curve: "circle", adjacentTo: { faceNormal: "+z" } });
```

Evaluate it with parcad as written and check that the part it builds is what
the brief asks for. If it is not, fix the script and evaluate again.

Rules: the volume must be one parcad reported for the part you settle on, not
a hand calculation. End with a one-line verdict in exactly this form:

VOLUME = <mm³>

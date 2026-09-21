# Selectors, second attempt

A proposal, not a plan. It comes from one session (2026-09-12) in which a model
built a VESA-mounted V holder for a 16" MacBook Pro over the CLI, and every
hour lost was lost to choosing edges. The part is `~/Documents/parcad/v-holder`
on the machine it was built on; the cost is itemised in
[DSL_GAPS.md](DSL_GAPS.md) §9. This document is about why the cost is
structural and what would remove it.

## What is wrong, in one sentence

**A query runs over every edge of the whole solid and measures extremes against
the whole document, while every real instruction is about one feature.**

"The outer corner of the lip." "The seam where the arm meets the hub." "Every
outside edge of this block." "The top rim of the pocket." Not one of those names
the whole part, and the language has no way to name less than the whole part
once two shapes have been combined. `generatedBy` reaches a boolean's *section
edges*, which is one of the four cases and does not compose with a direction.

The consequence is a working pattern that every long script here settles into:
**treat the primitive before you combine it.** The V holder's corner block gets
four compact selectors as a lone box, with four radii, and only then joins the
body. That works until the edge you want only exists *after* the join, which is
what a seam, a lip rim or a pocket mouth is. Then the author is back to global
extrema and a count they discover by failing.

Three concrete failures from the session, each a build-and-bisect cycle:

| wanted | written | got |
|---|---|---|
| soften the part's upright outside edges | `edges("\|Z")` | 36 edges including ones tangent to an earlier fillet; no radius builds, and the message names a count |
| round the two lips' top rims | `edges({ at: { z: "max" } })` | works, with `expect({ count: 20 })` found by writing 16 first — the count carries no meaning |
| blend the arms into the hub | `edges({ generatedBy: "spine", curve: "line" })` | works by luck: the seams happened to be the only straight edges that union created |

And two things that could not be said at all: "vertical *and* straight" (the
object form has no direction, the compact form has no curve kind), and
"these *or* those".

## The proposal

Three changes, in the order they pay back.

### 1. Edges know their angle and their length — **DONE**

Landed as described below, the same day. `dihedral`, `parallel` and
`longerThan` in both languages; smooth edges left out of treatments unless
asked for; every treatment failure and expectation mismatch lists its edges.
Cases: `edges-by-angle`, `smooth-edges-left-out`, `refuse-all-smooth`. The
V holder's three fillet failures would each have been one edit.


`describe_edge` gains the dihedral angle between the edge's two faces, measured
on the material side, and the edge's length. Two queries follow:

```js
edges({ dihedral: "convex" })      // an outside edge — "break every edge"
edges({ dihedral: "concave" })     // a seam, an inside corner
edges({ dihedral: "smooth" })      // tangent-continuous, within 1° of flat
edges({ parallel: "z", longerThan: 3 })   // upright, and no slivers
```

**Smooth edges are excluded from a treatment unless asked for.** They are the
boundary of an earlier fillet, and rolling a ball along one is the single most
common way a fillet here fails. Nothing about the shape changes; the default
just stops selecting what cannot be treated.

Every treatment failure and every `expect` mismatch then prints the edges it is
talking about, worst first: centre, length, angle, the two face kinds. "36 edges,
no radius builds" becomes "edge at (132.9, −80.5, 0…10), 1.5 mm long, between
two planes at 90°", and a three-round bisection becomes one edit. DSL_GAPS §2
asked for `convex` a month ago; this is the same request with the evidence
attached.

Cost: a day, and it was. `describe_edge`, `EdgeQuery` in both languages, the
treatment target resolver, and the two error sites; `eval/selectors.json` was
untouched because the compact grammar did not change.

### 2. Faces carry their name through booleans — **DONE**

`EdgeLineage` now carries `faces_by_source` beside its edges: a tag names
every face of its node's result, and the same OCCT history that follows edges
follows faces through union and cut, through fillet and chamfer (the vendored
`Treatment` keeps the builder alive for the question), and through rotate,
scale and mirror by moving the tracked faces the same way and matching them
by geometry. A fillet's new faces take the names of the faces its edge lay
between, and the same-domain merge after every boolean is followed through
its own history rather than assumed. Lost through offset, shell and
intersection, which report no history, and the refusal says so. Cases:
`scoped-corner`, `local-extrema`; GOTCHAS records the two ways it failed
first. One consequence to know: a face the unify pass merges from two
features carries both names, so across a coplanar join `on` reaches into
the neighbour — `local-extrema` widens one block to keep the faces apart,
and says why.


A `tag` on a node names its *faces*, and the name survives union, cut,
intersect, fillet and chamfer. OCCT reports `Modified`, `Generated` and
`IsDeleted` for faces exactly as it does for the edges `EdgeLineage` already
follows; this is the same relation one topology type up. A fillet's new faces
take the name of the faces whose edge they replaced.

That makes a tag mean in the B-rep backend what
[ARCHITECTURE.md](ARCHITECTURE.md) already says it will: a set of faces. Nothing
about scripts changes yet. Cost: most of a week, in `backend.rs` and the vendored
history wrapper; the hard part is deciding what a face made from two named faces
is called (both, is the answer that never lies).

### 3. Queries are scoped, and extrema are local — **DONE, in part**

`on` and `between` landed as below, and with `on` the `at` extrema are
measured among the surviving candidates. `not` landed 2026-09-21 — one
sub-query, subtracted before the extrema are taken, refused two levels deep
or empty — and with it the compact form's refusal now says that negation is
in the query form and that `check_selector` parses one for nothing: a session
spent two round trips and 3,346 bytes on `">Z and not |Z"` against a message
that named three spellings and stopped (docs/COIN_HOLDER_REVIEW.md, L2). Not
landed: `facing` (still `adjacentTo.faceNormal`), `any`, and a first-class
`faces()` query; the compact string keeps its document-global meaning rather
than lowering to the object form. The V holder's neck blend is now `between: ["spine", "fan"]` and
its lip rims `{ on: "cup", at: { z: "max" } }`, which is the reading test this
section asked for.


With named faces, a query can say where to look:

```js
edges({ on: "lip", facing: "+z" })                       // the lip's top rim
edges({ between: ["arm", "hub"] })                        // the seams, and only the seams
edges({ on: "block", parallel: "z", at: { x: "max", y: "min" } })  // its outer corner, after the union
edges({ on: "pocket", dihedral: "concave" })              // the pocket's floor edges
edges({ on: ["arm", "hub"], dihedral: "convex", parallel: "z" }) // upright outside edges of two features
```

`on` restricts candidates to edges bounding a face with that name; `between`
requires one face from each name — the seam selector, which is also what
`union({ blend })` computes internally and could then expose. **`at` is measured
within the candidates that survive the other terms**, so "the lowest of the
block's vertical edges" is sayable. The object form becomes complete, with
`parallel` and `facing` as the names for what the compact form spells `|Z` and
`adjacentTo.faceNormal`, and the compact string lowers to it:
`"|Z and >X"` is `{ parallel: "z", at: { x: "max" } }`. One specification, the
same two parsers as today, `eval/selectors.json` still the corpus.

`any: [q1, q2]` covers the rare *or*, and is the half of L2 still open:
`not` is in, `or` is not, and the refusal says so rather than leaving a
reader to find out. `faces(q)` becomes a first-class query
with `.edges()` on the result, because "the top of the lip" is how people and
models name things and a face is what a tag now is.

Cost: a few days once (2) exists. The examples corpus is the acceptance test:
every part still builds, and the ones that treat a primitive before combining
it can be rewritten to treat the feature afterwards, which is the reading test.

## What this does not fix

A 1.5 mm sliver where an arm's face crosses a block corner is geometry. No
selector removes it. What (1) does is name it in the error, and a
`longerThan: 3` term keeps it out of a cosmetic pass.

Full assemblies, the laptop as a reference body, and the fit check are a
different document; see DSL_GAPS §9.

## What stays

`expect({ count })` stays. It caught seven of twelve parts in the first corpus
and four of five treatments in this one. The change is that when it fires, the
message shows the edges rather than the number.

# A tag on a union of many solids: diagnosis (2026-11-05)

docs/NEXT.md, "A tag on a union of many curved solids makes it crawl": the
planter saucer (a dish fused with 37 revolved bumps) built in 1.3 s untagged
and ran past 120 s with `.tag("saucer")` on the union. Queue item 1. This note
is the measurement behind the fix; the durable rule goes to GOTCHAS.md with
the fix.

## What it is

`EdgeLineage` keeps `faces_by_source: BTreeMap<String, Vec<Face>>` (and
`by_source: Vec<Edge>`) — a *list*, never a set. A boolean carries each
listed face through `BooleanShape::modified_face`, and the unify pass after
it through `Unification::modified_face`; the tag on the node then appends
every face of the result (`through_boolean_all`, `faces_by_source[tag]
.extend(result.shape.faces())`). When a fuse splits a face on the same domain
— the dish's bottom plane and a bump's bottom disk are coplanar — OCCT's
`Modified(face)` returns *both* pieces, and `UnifySameDomain` then merges the
pieces back into one face, so both pieces map to the same face: one entry
became two entries naming one face. Nothing collapses them, so the next
boolean step maps each of the two again. The tag's list doubles at every
fuse that touches that face: 2^k entries after k successive booleans, each
step calling `modified_face` once per entry.

The saucer at the commit that recorded the crawl (9662c4b5, 2026-09-15)
fused its 37 bumps one at a time, so the tag's list reached ~2^37 entries.
A `sample` of the worker 8 s into the tagged build (300 s timeout, stopped
in "union node 38 (saucer) with node 28") puts 100 % of the main thread in
`build_node_afresh`: 65 % under `unified_tracked` → `EdgeLineage::
through_unify` → `Unification::modified_face`, 33 % under
`EdgeLineage::through_boolean` → `BooleanShape::modified_face` — all of it
`std::vector<TopoDS_Shape>` construction and `TopoDS_Face` handle
allocation and release, i.e. the per-entry cost of a list far too long. The
boolean itself does not appear.

## Why the saucer no longer shows it, and what still does

`boolean_in_layers` (2026-09-16, "Tools whose boxes are clear of each other
go into one OCCT boolean") fuses every non-overlapping tool in one layer, so
the saucer's 37 bumps are one boolean and one `through_boolean_all`: at HEAD
the tagged saucer builds in the untagged time. Tools whose boxes overlap
still go into successive layers, each carrying `tag`, and the list doubles
per layer. HEAD repro (`target/iterate/parcad`, median of one, M4):

A 60×60×2 plate fused with n posts stacked at one spot, every post's bottom
coplanar with the plate's bottom, so every post is its own layer.

| n posts | untagged build | tagged build |
|---|---|---|
| 8 | 31 ms | 17 ms |
| 12 | 23 ms | 28 ms |
| 16 | 30 ms | 97 ms |
| 20 | 38 ms | 1479 ms |
| 22 | 43 ms | 5028 ms |
| 24 | 47 ms | 20652 ms |

×3.4–4.1 per two posts: 2^n. The same law on the Sep 15 build of the
saucer, bumps fused one at a time: 7 bumps 52 ms, 19 bumps 713 ms, 37 bumps
over 300 s.

The Sep 15 build was made in `.claude/worktrees/saucer-bisect` with
`PARCAD_OCCT_PREBUILT` pointed at HEAD's OpenCASCADE install (patches
0003–0007 are not in that commit; none touches booleans or the unify
history, and the untagged saucer built in 1.4 s against the 1.3 s recorded).

## What it is not

- Not `tag_extents`. The `AddOptimal`-per-face loop that 6fb78d20 pruned
  the same evening was the first suspect: a harness over the saucer's 44
  faces read from its STEP export (`BRepBndLib::AddOptimal`, no
  triangulation, no tolerance) takes 16 ms for the B-spline bumps and 37 ms
  for the current spherical domes, all faces together.
- Not the boolean: `kernel_ms` of the tagged and untagged saucer agree on
  every build that finishes, and the sample shows no `BOPAlgo` frame.
- Not the host or the client: both idle under `sample` while the worker
  ran. (My first timing runs here measured their own `sleep`s — a 47 s
  "tagged" number was 46 s of sampling delays; the call had taken under a
  second.)

## HEAD timings for the record (iterate profile, median of 5 wall ms)

| part | untagged | tagged |
|---|---|---|
| saucer, current dome bumps | 311 | 252 |
| saucer, B-spline bumps as of 9662c4b5 | 567 | 574 |
| planter + saucer as of 9662c4b5 | 3167 | 3156 |

## The fix, and what it measured

A tag names a *set* of faces and a set of edges. `EdgeLineage::unique` keeps
each name's list unique by sub-shape identity (`Identity` in the vendored
crate: `TopoDS_Shape::IsSame`, same `TShape` at the same location, hashed as
OCCT's own shape maps hash) after every history it is carried through and
after every tag written; `boolean_in_layers`' seam edges the same. A face
listed twice already meant one face everywhere a reader asked (`face_tags`
writes each tag once per face index), so no tag's ownership moves.

Release profile, `tools/bench-kernel.sh` (median of 3, build ms), before
and after:

| part | before | after |
|---|---|---|
| tagged stack, 26 posts (`eval/cases/tagged-stack`) | refused at the 20 s budget | 53–127 |
| tagged stack, 24 posts | refused at 20 s (20652 under a 300 s timeout) | 69 |
| untagged stack, 24 posts | 46 | 56 |

The ordinary parts were read under a load average of 16–27 from other
sessions, so they were paired: the pre-fix and fixed release CLIs
alternated run by run on the same graph, five runs each, medians (build ms,
A pre-fix / B fixed): bracket 66 / 66, timing-pulley 212 / 212, plate-stand
1053 / 1084, twisted-planter 2670 / 2672, untagged stack 46 / 45. Corpus:
221 passed, 0 failed, the new case included, every recorded tag extent and
face count unchanged.

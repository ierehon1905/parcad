# Section checks: three layers, one corpus

A section — a closed outline of corners, arcs and curves — is judged three
times before it becomes a face:

| layer | where | what it may decide |
|---|---|---|
| DSL | `checkSection` in `app/src/dsl.ts` | the shape of each entry: its keys, its numbers, its point count |
| core | `SectionEntry` parsing and `resolve` in `crates/parcad-core/src/section.rs`, `Op::validate_outline` in `graph.rs`, `section_crossing.rs` | how entries combine, and the geometry of every line, arc and non-fitted curve |
| kernel | `section_wire` and `checked_face` in `crates/parcad-occt/src/backend.rs` | what only a built curve shows: fits, insets, and `BRepCheck` as the backstop |

When two layers disagree, an author hears from the wrong one about the wrong
thing. This page is what a seeded fuzz of all three found, what was changed,
and what is meant to stay different. `eval/sections.json` holds it down, the
way `eval/selectors.json` holds the selector grammar.

## Running it

```bash
tools/section-fuzz.sh [--seed N] [--per-family N] [--family NAME] [--keep DIR]
bun tools/section-promote.ts DIR/verdicts.jsonl > eval/sections.json   # accept a change
```

`tools/section-fuzz.ts` writes outlines in 24 families — polygons simple,
shuffled and clockwise; slots whose arms nearly touch; arcs and curves sagging
onto an edge; near-collinear corners and arcs; edges from 1e-3 to 0 mm; the
same shapes from 1e-6 to 1e4 mm; rounds, re-entrant rounds, `through` and
`radius` arcs whose radius is barely enough; full circles; splines between
corners and closed, sparse and dense and noisy; Béziers, B-splines, open and
closed fits; curves diving through the far edge; 47 hand-written malformed
outlines; and every `pinned/` outline already in the corpus — with the DSL's
verdict on each. `examples/section_fuzz.rs` in `parcad-occt` adds the core's
verdict and the kernel's: lowering an extrusion of the outline
(`backend::build_part`, where every section check runs; meshing is left out, a
20 m outline at the fixed 0.01 mm deflection only times out). Where the core
refuses an outline it can still resolve, the kernel is also asked alone —
`resolve_unchecked` then `section_face_verdict` — so a core stricter than the
kernel shows up too. Outlines are judged in a child process restarted past any
it aborts on.

The gate is the corpus, read by three tests: `agrees_with_the_shared_section_corpus`
in `section_crossing.rs` (core, in `check.sh --fast`), `section-corpus.test.ts`
(DSL, in `bun test`), and the test of the same name in `backend.rs` (kernel,
`--features kernel`). A kernel case slower than 150 ms is left to the fuzz run.

## Measured

Seed 1, 100 per family, this machine with five other agents building beside it.

| | before | after |
|---|---|---|
| outlines | 2246 | 2247 (+ the pinned lamp section) |
| no disagreement at all | 1434 | 1416 |
| **worker aborted on an outline the core passed** | **16** | **0** |
| kernel alone aborted on an outline the core refused | 13 | 0 |
| crossings refused only by `BRepCheck`, with no location | 281 | 4, now with a sampled location |
| crossings the kernel **builds**, found by the core and confirmed by an independent dense sampling | not seen | 5 (+ the lamp) |
| DSL refuses what the core accepts | 1 | 0 |
| per-entry mistakes the DSL let through to the core | 34 | 0 |
| fit and inset refusals only the kernel can make | 164 | 164 |
| core refuses what the DSL leaves to it | 336 | 662 |
| harness wall time | 16.5 s | 16.6–30 s (load) |
| core check time, all outlines | — | 0.36–0.48 s; worst single outline 19 ms (a 250-point closed spline) |
| gate added | — | core 0.01 s, bun 0.02 s, kernel 0.27 s (release) |

The corpus is 112 outlines, 85 kB.

## The classes, and who was wrong

**The worker aborted on two kinds of polygon the core passed** — the kernel was wrong
to die and the core was wrong to hand it over. A slot whose walls are 1e-7 to
1e-9 mm apart, and a 1e-8 mm edge, both take OpenCASCADE past its confusion
distance (1e-7) inside the face builder, which terminates the process. The core
tested touching at `1e-9 × scale`, 3e-8 on a 30 mm part. Fixed in the core:
`section_crossing::RESOLUTION_MM` (1e-6) is now the floor of that test, and two
corners or arc ends closer than it but not the same point are refused by name
("corners 3 and 4 are 1.0e-8 mm apart…"); an exact repeat is still dropped, as
before.

**Arcs and curves that cross were diagnosed by `BRepCheck` alone** — right
verdict, wrong layer, no location ("an arc or curve runs into another edge").
The core now checks every line, arc and non-fitted curve against every other on
the exact curves (`section_crossing.rs`): each piece is held by the control
polygon that contains it — a Bézier span's poles, an arc's ends and tangent
corner — and pairs are halved until their hulls part or both are below the
resolution. The refusal names both pieces in the author's terms and where they
meet: "the spline between corners 2 and 3, between its point 1 and corner 3
meets the straight edge between corners 3 and 0 near [0.0000, 9.1992]".

**`BRepCheck_Analyzer` passes crossings.** The finding that matters most. Five
fuzzed outlines and the lamp section below cross themselves, the kernel's exact
check on the face accepts them, and the extrusion builds. All six were
confirmed by dense sampling written separately from the core's code. Each is
a curve arriving at or leaving a corner and swinging across the edge beside it
within a millimetre of that corner, or a spline turning sharply at one of its
own points and looping there. The core now refuses them; the kernel check
stays as the backstop.

**Neighbours meet at their corner at every scale.** The first version reported
every corner sharper than about 4° as a contact, because two edges at a small
angle are within any tolerance of each other for a length that grows as the
angle shrinks. Pieces that share a corner are therefore held to exact hull
overlap, and the half-pair holding the corner is never a contact: what is left
to find is a fold, where they overlap away from it. A corner 0.001 rad short of
a fold builds; an edge that runs back along the one before is refused. A
*curved* cusp — a half circle leaving a corner straight back down the edge it
arrived on — touches only at the corner and builds, as the kernel builds it.

**Fits are the kernel's.** The core cannot know a fitted curve; it has only the
points. 157 fits that cannot hold their tolerance and 6 that loop between
points are refused by the kernel with the tolerance that would hold, which is
the right layer. The 4 where a fitted curve runs into *another* edge used to
reach the generic `BRepCheck` message; `locate_crossing` in `backend.rs` now
refits, samples the whole outline and names the two pieces and the place.

**The DSL checks entries, the core checks combinations.** `checkSection` says
so, and the fuzz found it leaving per-entry facts to the core: an empty
`spline`/`bezier`/`fit` list between corners, a Bézier past degree 25, a
`bspline` degree past 25 or with fewer poles than it needs, a zero `start` or
`end` direction. Those are now the DSL's, in the core's words. Everything that
depends on two entries or on geometry — rounds that overlap, an arc too short
for its chord, a crossing — stays with the core, and the 662 "DSL accepts,
core refuses" are that division, not a defect. Duplicating geometry into
TypeScript would be a second implementation to keep in step for no gain in
what the author reads.

**`{ at, round: 0 }`** was refused by the DSL and taken as a sharp corner by the
core. The DSL's rule is the documented one; the core now refuses it too and says
to write `[x, y]`.

**The core stricter than the kernel, on purpose.** Two outline edges closer
than 1e-6 mm (16 outlines), and outlines under 1 µm across (4, now refused as
"only 2.0e-6 mm across… scale the outline up" rather than as a crossing), build
in the kernel alone but are not solids anything downstream can trust. Two
corners, and three collinear corners, are refused by the graph and would give
the kernel a face of no area.

## The lamp

Section 0 of `repro/lamp4.js` (fitted-sections worktree), written as one closed
`spline` through its 200 points, builds in the kernel. The cubic crosses itself
0.07 mm from point 130. The core now says "the closed spline between its points
129 and 130 meets the closed spline between its points 130 and 131 near
[-19.9877, -27.5765]", and suggests `{ fit }`. It is kept in the corpus as
`pinned/lamp-shade-section-0-spline`; the generator reads pinned outlines back
out of the corpus, so regeneration keeps them. The other 14 sections of that
lamp are simple and build.

## Not done

- Only extrusions are fuzzed. Revolve, loft and sweep sections go through the
  same `resolve`, so they get the same checks, but their own rules (the axis,
  section pairing) are not in the corpus.
- A fitted curve against another edge is found by sampling, after the kernel
  refused; a crossing between sample points is still reported by `BRepCheck`
  alone, and a crossing `BRepCheck` misses is not looked for.
- The search gives up after 200 000 splits and accepts, leaving the question
  to the kernel. No fuzzed outline came near it.

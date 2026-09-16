# What OpenCASCADE's validity check does not catch, and what does

`BRepCheck_Analyzer` judges each face against its own boundary and each shell
for closure and orientability. It does not ask whether two faces of a solid run
through each other, or which way the solid faces. The section fuzz found it
passing self-crossing outlines (docs/SECTION_CHECKS.md); this page is the same
question asked of every place that used it, or nothing, as a gate on a built
shape.

## The gates

| where | relied on to catch | caught before | now | added cost |
|---|---|---|---|---|
| `checked_face`: section faces (`BRepCheck`, exact) | a fitted curve running into another edge | only where `BRepCheck` refused, then located by sampling | unchanged: a fuzz of 80 fits swinging across the edge beside a corner found no crossing `BRepCheck` missed | — |
| `check_blend`: a blended union or cut (`BRepCheck`) | a blend whose surface crosses itself or a neighbour | 2 of 400 fuzzed blends accepted wrong; 6 caught only by the mesh backstop, whose message names other causes | refused by name, with the largest radius measured to build | the changed-face check below |
| fillet and chamfer nodes (growth check only; no `BRepCheck`) | a treatment cutting through the wall behind it, or rounds running into each other | 3 chamfers accepted wrong; 2 fillets caught only by the mesh backstop | refused by name, with a size measured to build | the changed-face check below |
| `attempt_treatment`: the probe below a failed size (`BRepCheck` + growth) | that a suggested size is sound | sizes measured on an input earlier probes had altered (a vertex left at 42 mm tolerance); a self-crossing size could be suggested | each attempt on its own copy of the input, held to the changed-face check too | refusal path only |
| loft (bounding-box bulge; volume sign) | walls passing through each other | a half-turn loft, a star turned three corners on (built with volume −3382 mm³), a curved loft through its axis — all accepted; an L turned past its notch caught only by the mesh backstop | ruled walls between polygons: exact, in the graph (`loft_walls`); curved or smooth: the kernel's self-intersection check on the loft | exact check ≈ 0; smooth/curved lofts ≈ one check each (loft-smooth 17 ms) |
| sweep (graph: bend radius, helix pitch; bounding-box bulge) | a path that comes back across itself | four sweeps and pipes accepted; one caught by the mesh backstop | `spine_contact` finds where the spine comes back within twice the section's reach, and only then the self-intersection check runs | ≈ 0 unless the path comes back near itself |
| shell (bounding box of the cavity) | a cavity that is not the inward offset | 37 of 80 fuzzed shells refused as "the operations cancelled all the material away": the kernel returned the cavity as a bare shell | the cavity closed into a solid, turned outward, self-intersection checked, and the result held to part volume minus cavity volume | one check per shell |
| offset (bounding box; volume sign) | a grown solid that is empty, turned or crosses itself | empty results refused as "inf mm from where it must be" | closed into a solid, turned outward, self-intersection checked | one check per offset |
| orientation: loft, sweep, curved extrude, thread, offset (volume sign); nothing after the last node | an inside-out solid | a walled smooth loft (integration, 2026-09-16) passed every gate; a point outside it probed as material | `facing_outward` at every operation that builds a solid from surfaces; after the last node, each closed mesh shell's winding checked against its nesting (`Tessellation::inward_shells`); probes and fit checks, which make no mesh, ask the B-rep (`check_finished`) | per-operation classification replaces the volume integral it used; the mesh check is free |
| mesh backstops in `serve::measure` | a surface that does not close; a closed mesh of the wrong solid | unchanged | unchanged, plus the orientation backstop above | — |
| `validity_probe`, `heal_probe` | nothing: diagnostics under an environment variable | — | unchanged | — |
| booleans | nothing | — | unchanged; they alter their arguments' tolerances in place too, by at most 4.7e-7 mm over the corpus, below every check's resolution | — |
| `probe_step`: a foreign STEP file | nothing: it measures, it does not accept | — | unchanged | — |

## The check on what changed

`Shape::self_interference_since(before)` runs `BOPAlgo_CheckerSI` on the
faces a fillet, chamfer or blend made or trimmed — those of the result that
are not faces of the shape it was given — and every face whose box meets
theirs. Faces the operation left alone met nothing before it. A subclass of
the checker keeps only the candidate pairs with a changed face on a side, and
intersects only the changed faces with themselves. That last pass is most of
the cost on a B-spline blend, and it is not optional: `refuse-folded-blend-corner`
is a 0.4 mm blend whose corner patch folds over near its degenerate corner
(its normal, sampled from its own poles, turns over on 224 of 40,401 points),
no two faces cross, the mesh closes, and nothing else sees it.

A treatment builds on a topology copy of its input (vendor/opencascade
PARCAD-CHANGES.md), so "not a face of the input" is the copy's faces, and so a
refused attempt cannot leave widened tolerances on shapes something else holds.
Before the copy, the check read 130 vertex contacts on a sound 1.13 mm blend
after the refused 1.5 mm attempt.

## Measured

Fuzz sets, generated by seeded scripts, each graph judged in its own process
by `examples/validity_audit.rs` (the whole worker pipeline, plus
`BRepCheck` plain and exact and `BOPAlgo_CheckerSI` on the finished shape),
against the unmodified worker:

| set | graphs | accepted wrong before, refused now | caught before only by the mesh backstop, named now | built before and now | refused before and now |
|---|---|---|---|---|---|
| hand-written defects | 29 | 8 | 2 | 5 | 13 |
| treatments and blends (seed 9001) | 400 | 5 | 8 | 125 | 259 (+3 still by the mesh backstop) |
| shells and offsets (seed 4242) | 160 | — | — | 87 + 37 shells that were refused | 33 (+3 crash before and now) |
| fits near a corner (seed 11) | 80 | 0 | 0 | 25 | 55 |
| the eval corpus | 132 | 0 | 0 | 106 | 26 |

The new refusals were confirmed apart from OpenCASCADE's checks: the part
built with the check bypassed, meshed at 0.01 mm, and its triangles tested
pairwise for crossings near the reported point — 26, 18 and 26 crossing pairs
on the three chamfers, 25 on the blend, 12 and 20 on the two path sweeps, 176
on the tapered pipe, 298 on the curved loft, 393 and 398 on the two spline
pipes. The folded corner was confirmed from its patch's own poles; the star
loft by the negative volume it built with; the half-turn loft is four walls
through one axis by construction.

The 37 shells that now build were checked at 2,800 random points:
each is material exactly where the base is and within the wall of its
surface, with no exception; `shell-filleted-box` and `shell-of-a-union` match
their closed forms to eight figures.

No corpus case changed its verdict. `refuse-unblendable-junction` names
1.13 mm, as before — but before, that number was measured on an input the
refused probes had altered.

Cost, the whole worker pipeline per corpus graph, old and new worker
alternated, best of three, this machine at load 14–31:

| | before | after |
|---|---|---|
| all 132 graphs | 30.51 s | 32.19 s (+5.5 %) |
| the 106 that build | 29.18 s | 30.42 s (+4.2 %) |
| median graph | | +2.8 % |
| largest increases | | section-m10 +188 ms (+9 %), untitled2 +177 ms (+5 %), plate-stand +149 ms (+8 %), pipe-tee +117 ms (+53 %), wash-bottle +114 ms (+6 %), cast-foot +62 ms (+58 %) |

The self-intersection checks take 0.68 s of 13.6 s of corpus builds, timed
inside the worker; the orientation classification (which replaced a volume
integral at five of its sites), the treatment copy and the mesh's orientation
reading make up the rest, and were not timed apart. The
relative outliers are the blend-heavy parts, where a changed B-spline face has
to be intersected with itself.

## Not done

- A fillet or chamfer node does not run `BRepCheck`, as a blend does, nor
  classify the part's orientation. Of 400 fuzzed treatments, one fails
  `BRepCheck` and one leaves a second shell enclosing material; both are
  refused by the mesh backstop, with its generic message. Fed into a cut or a
  union, both still failed the mesh backstop (4 of 4 tried), or were cut away
  whole (1).
- The walled smooth loft that prompted the orientation gate is on
  `engine/wall-loft`, not this branch; the gate was tested on solids reversed
  on purpose and on the offset of a filleted body.
- `BRepCheck` remains the backstop in `checked_face` and `check_blend`; it
  still catches what it catches (a surface that will not close) and is cheap.

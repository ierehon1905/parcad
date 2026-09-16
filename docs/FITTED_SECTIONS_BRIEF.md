# Brief: fitted curves and outline insets

One task, two ops, both blocked on the same missing idea: parcad can draw an
exact curve it is *told*, and cannot take a curve it is *given*. Everything
generative — a simulation, a scan, a contour, an involute — arrives as points.

Written for a single session. Read this, then `docs/ARCHITECTURE.md` and
`docs/GOTCHAS.md`. The evidence below was measured, not assumed.

## What is missing, measured

A reaction-diffusion lamp section (200 points, ~10 lobes, 135 mm across) can
enter the DSL three ways today, and each is wrong in its own direction:

| entry | result | measured |
|---|---|---|
| corners | builds | 2402 planar faces; visibly faceted |
| `spline` | **refused** | the interpolating cubic overshoots into itself: `BRepCheck_SelfIntersectingWire` |
| `bspline` | builds smooth | the built curve is **up to 1.01 mm from the points** (mean 0.22), and nothing reports it |

The third is the one the project's own rule is against — not because it
approximates, but because the approximation is silent. `deviation.js` in the
session scratchpad measured it with a de Boor evaluator written on the side,
which is exactly the work a part author should never have to do.

Cost, same part: ruled loft of polyline sections 7.8 s; `smooth: true` through
200-pole B-spline sections **51 s**, and a 47 MB STL at 0.01 mm deflection. The
slowness is the missing feature wearing a different hat — 200 poles to describe
ten lobes.

`docs/DSL_GAPS.md` already records the same wall from the other side: an
involute gear is blocked because "a spline through sampled involute points is
the approximation this refuses". One entry closes both.

## Op 1 — `{ fit: points, tolerance }`

A section entry that fits a B-spline through sampled points, **measures** the
worst deviation, and refuses if it exceeds the tolerance.

- **Prior art, already vendored, unbound**: `GeomAPI_PointsToBSpline(points,
  DegMin, DegMax, Continuity, Tol3D)` in
  `target/.../occt-sys-*/out/include/GeomAPI_PointsToBSpline.hxx`. Fitting is
  not ours to write.
- **Measurement, also unbound**: `GeomAPI_ProjectPointOnCurve::LowerDistance`
  gives the distance from each input point to the fitted curve. Report the max
  — `deviation_mm`, alongside `deflection_mm` in `PartReport`, measured and
  never the requested `tolerance`.
- Refuse when the fit cannot hold the tolerance, and when the fitted curve
  self-intersects — dense points can be simple while the curve through them is
  not, which is how `spline` fails today. The refusal names the fix: raise the
  tolerance, or thin the points.

Layers: bind in `vendor/opencascade-sys` (+ `vendor/opencascade/PARCAD-CHANGES.md`),
`SectionEntry::Fit` in `crates/parcad-core/src/section.rs` with its parse arm
and `VOCABULARY` line, the edge in `crates/parcad-occt/src/backend.rs`, the type
and docs in `app/src/dsl.ts`, `eval/cases/`.

## Op 2 — `inset(outline, d)`

The wall. A lamp shade is an outline and that outline stepped inward; today a
part must hand-roll it, and every hand-rolled version folds in the valleys of a
lobe. Measured this session: a per-point normal offset self-intersects, and
**denser sampling makes it worse**.

- **Prior art, vendored, unbound**: `BRepOffsetAPI_MakeOffset` — planar wire
  offset with loop removal.
- Related and already known-bad: `Op::Offset` refuses inward outright
  (`backend.rs:3466`, "the B-rep backend only grows so far"), and `shell()` is
  `solid − offset_surface(-t)`, which refused this part. Neither is the route.
**Decided: this belongs in the kernel, not the DSL.** `line2d` and `hull` are
precedent for plane arithmetic in a script, and they are the wrong precedent
here, for two measured reasons.

- A script can only offset a *polygon*. Once `{ fit }` lands the outline is an
  exact B-spline, and a script-side inset would have to sample it back to
  points, offset those, and re-fit — approximating twice around a curve just
  made exact, and leaving `measure_wall_thickness` measuring a wall that is
  approximate by construction. The kernel offsets the curve itself.
- A script-side inset cannot refuse. When it folds it emits a broken outline,
  and the kernel's complaint is then about the *section* crossing itself, which
  is a diagnosis away from the cause. This session lost an hour to exactly
  that. Kernel-side, the refusal names the inset and the distance that would
  fit.

Performance says the same: script code runs inside the 5 s sandbox that is
already the binding constraint, and staying accurate in a script needs dense
points — which is what produced the 51 s loft and the 47 MB STL.

**The guard it needs.** OCCT offsets lie silently: `offset_slip` in
`backend.rs:114` exists because `offset_surface` on a boolean "returns only the
post, with no error". Assume `BRepOffsetAPI_MakeOffset` does the same on some
wires. Measure the result — area against the closed form on shapes that have
one, and the inset curve's own distance back to the original — and refuse on
disagreement rather than returning a wire that is quietly wrong. A dropped or
folded offset that builds is worse than a refusal.

## Op 3 — the script budget

`crates/parcad-host/src/script.rs:42`, `DEADLINE = 5 s`, not configurable. A
generative part cannot ask for more: this session had to write a counting-sort
spatial grid *inside a part* to fit, and still lost 25-section variants to it.
Either raise it, or let a call ask the way `evaluate_part` takes `timeout_s`.
It is a sandbox boundary, so this is a policy decision, not just a constant.

## Proof required

Nothing here is done because a render looks right.

- A case per op in `eval/cases/`, each against a closed form: a circle sampled
  at 200 points and fitted must come back within tolerance of the analytic
  circle; an inset square's area must equal the closed form.
- The deviation must be *read back* from a built part, not computed in the test.
- The lamp: `field/` in both thinking arms once `read_docs` describes the new
  entries — a model that cannot find `fit` has not been given it.
- The honest end state is the lamp as a 1.6 mm shade whose wall thickness is
  measured by `measure_wall_thickness`, not asserted.

## What is not in scope

- Surfaces. BeeGraphy's lamp is a lofted *surface* with no thickness; parcad
  measures volume, wall and watertightness, and that stays.
- Winding-rule fill for self-overlapping outlines. Real gap, recorded, later.
- Materials and lighting in renders. Presentation, and out of scope by
  decision.

## Reproduction

`repro/` in this worktree (untracked, never commit it):

- `lamp3.js` — polyline sections, 2402 faces, 3.3 s. The faceted baseline.
- `lamp4.js` — `bspline` sections, 30 faces, 7.8 s.
- `lamp5.js` — the same with `smooth: true`: 4 faces, **51 s**.
- `deviation.js` — the de Boor evaluator that measured the 1.01 mm. When
  `{ fit }` lands, this file should become unnecessary; that is the test.

Run them against a host built from this tree, not the installed app — the
installed one predates spline sections and rejects them:

```bash
tools/build-worker.sh && cargo build --locked --release -p parcad-cli
PARCAD_HTTP_PORT=4343 PARCAD_OCCT_WORKER=$PWD/target/release/parcad-occt-worker ./target/release/parcad serve
```

A second checkout would pay a cold OCCT build; point `PARCAD_OCCT_PREBUILT` at
the existing `target/release/build/occt-sys-*/out` instead.

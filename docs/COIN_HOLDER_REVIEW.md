# What one coin-holder session says to build next

A review of Claude Code session `0a039327-b978-439a-ad6f-f5b8287c3109`
(2026-09-18, 21:14–22:08): a user and a model designing a 3D-printable pocket
euro coin holder over the parcad MCP. Six user turns, five designs from
scratch, 78 tool calls, 33 `evaluate_part`, 241 KB of script sent, 191 KB of
replies read, nothing exported, nothing printed. The final part is saved as
`euro-coin-holder`; the version the user said "nice" to (8.3 cm³, one piece)
survives only in `.history`.

Three reviewers read the transcript from three lenses; their full reports are
appended below. This section is the synthesis and the order.

## What the session measured, and what it asserted

Every geometric claim was measured and the kernel caught real defects: 35°
knife edges at the C-arm tips, a catch reaching 0.6 mm into a 0.6 mm slot, four
fillets that could not build. Every mechanical claim was a guess: "needs no
supports" (five times, never measured; the user found the overhangs by eye),
"the arms are shaped to flex" (no stiffness, no strain), "print in PETG, PLA may
snap on the first press" (no material model). Three of the six user turns were
mechanics questions the tools could not touch. parcad is a geometry witness and
not yet a mechanics witness.

Where the time went: 45 script sends totalling 241 KB, nineteen of them
changing under ten lines; 11 round trips lost to language and tool-surface
errors (one silently wrong: `section.offset` was dropped and the cut went
through the middle); 8 of 33 builds were hand-built verification rigs
(coin stacks, tipped coins, a sliding coin) that then went stale — the model
wrote "not yet re-run on this version" twice. The single most telling line in
45 scripts: the model encoded a string's *character count* into a body's Y
coordinate so it could read a number its own script had computed.

## The order

1. **A one-day correctness batch.** `deny_unknown_fields` on every MCP
   request struct (a wrong measurement today, not just a retry); `Treatment.edges`
   in every reply (unlocks `.expect()`, used in 0 of 45 scripts); name the
   shadowed builtin on the reserved-word collision; validate `rotate`/`mirror`/
   `scale` arguments in `dsl.ts` and take "this host is too old" off plain type
   errors; `parcad call --set` coerces by the tool's schema and MCP accepts numeric
   strings; expire viewers nobody has heard from; dedupe `tags`; stop echoing the
   script from `open_project`/`set_script`. Language report L1–L5, L9, L10, L12.
2. **Edit in place.** A `project` argument on every script-taking tool, and
   `edit_part` (old/new, refused on ambiguity, evaluated before written,
   snapshotted). Cuts script traffic ~79 % and is the one change that halves a
   session. Language B1, workflow §7.
3. **Checks that live in the part.** A `checks` key on the returned object
   (`clear`, `interferes`, `wall`, `size`, `standsOn`), reference bodies that are
   measured and drawn but never exported or counted, and `note()` so a script can
   report a value it computed, segregated as requested-not-measured. Shares the
   export door with NEXT.md item 1. Language B2, workflow §2, mechanical §2.6.
4. **Overhang measured, and print files on save.** Per body, in its declared
   print orientation (`.printedUp()`), faces at or under the threshold with area
   and span, bridges as spans, support volume by one boolean; `save_project`
   writes `print/<body>.3mf` laid flat with colours, and the window gets a Print
   button. Answers user turn 3 and ends a session with files instead of a
   question. The third line of NEXT.md item 1. Mechanical §2.1, §2.8; workflow
   §3, §4.
5. **`between_bodies` says depth and contact, not just volume.** `depth_mm`
   on interference, `contact_mm2` on touching. A 0.002 mm³ graze was read as
   "securely blocked"; a corner touch was read as a seated stack. One day.
   Mechanical §2.3.
6. **The brief.** `brief({ envelope, budget_cm3, printer, ... })` recorded on
   the Doc and reported against on every build, with a `POCKETS` table; and a
   scale rule on every raster view. "But I said pocket" and "it's too big" were
   both requirement corrections the first reply could have made. Workflow §1.
7. **Vocabulary a printed part needs.** `MATERIALS` by filament name; an
   `OBJECTS` catalogue with profiles (the 20 c coin is a seven-lobed flower, not
   a circle; heat-set inserts as the first row); `fit(kind, nominal)` for printed
   clearances; `{ at, cut }` and `rect()` in sections; `not` in the query form;
   a `styles` topic with aliases, a gestures section and "do not fake hardware".
   Mechanical §2.5, §2.7; language L2, L6, L8; workflow §8.
8. **Exploration and history.** `compare_variants` (up to four scripts, one
   contact sheet, one table); `list_snapshots` carrying each version's size and
   volume, a version strip in the picker, and one sentence telling the model to
   fork rather than overwrite. Workflow §5, §6.
9. **The mechanics witness.** `check_motion` with a conservative-advancement
   certificate (replaces the four hand-built rigs); `check_flex` for straight
   cantilevers and circular arcs, refusing everything else, with a sourced
   per-axis material table — the only honest answer to "is pushing the thick
   plastic feasible?". Mechanical §2.2, §2.4, §2.9.

Two things not to build: a rubric the model grades its own render against
(PERCEPTION §7's failure), and a style doc per brand.

Each report below carries its own field cases, corpus cases and file-level
sizes. Two field cases are already written (workflow §11).


---

# Appendix A — mechanical / DfAM lens

The file write was blocked by the harness (subagents must return findings as text), so the report is below in full.

---

# What the euro coin holder session says ParCAD should build

**Lens:** mechanical engineering, design for additive manufacture, mechanism.
**Source:** the condensed transcript, all 45 evaluated scripts, the three renders the user saw, and the repo at `/Users/leon/dev/parcad` as of `d3fcda11`.

*(Note: I was asked to write this to `/private/tmp/claude-501/-Users-leon-dev-parcad/852cb4df-8daa-4e72-b0a9-35115e79228e/scratchpad/report-mechanical.md`; the Write tool refused the path on a report-file guard. Nothing in the repo was touched.)*

---

## 0. The shape of the session, in one paragraph

Seven user turns, five redesigns, 45 kernel calls, one saved part. Every *geometric* claim the model made was measured — volume, bounds, watertightness, wall thickness, body-to-body clearance — and the kernel caught real defects (35.3° knife edges at the C-arm tips, a catch that reached 0.6 mm into a 0.6 mm oversized slot, a fillet that could not build at 1.5 mm). Every *mechanical* claim it made was a guess dressed as a conclusion:

- "it sits flat on the bed in one piece and **needs no supports**" — five times across the session, never measured once;
- "the only part that gives is the thin C arms, which is what they're shaped for" — no stiffness, no strain, no force;
- "**I'd print it in PETG**, which bends without cracking where PLA may crack" — no material model anywhere in the system;
- "they should hold in PETG; **in PLA they may snap on the first press**" — the final sentence of the final answer, about the part's only structural member.

That asymmetry is the finding. ParCAD is already an excellent *geometry* witness and is not yet any kind of *mechanics* witness, so a model working in it produces parts whose shape is certified and whose function is asserted. Worse, the asserted half is the half the user kept pushing on: three of the seven user turns ("pushing the thick plastic", "coins fly out", "overhangs a 3d printer would struggle with") were mechanics questions the tools could not touch.

Two smaller structural findings run through everything below:

- **The model built its own instrumentation four times** and once resorted to smuggling a number out through geometry (§2.2, §2.6). Every rig was `between_bodies` used as a collision oracle.
- **`between_bodies` answers with the wrong physical quantity** for the questions it was being asked (§2.3). A shared volume is not a penetration depth and a touch point is not a contact patch, and the model read both as if they were.

---

## 1. Ranked proposals

Ranked by outcome per unit of effort. Effort is my estimate in Fable/Opus-session days on this codebase.

| # | proposal | effort | outcome |
|---|---|---|---|
| 1 | §2.6 `note()` — a script can report a number it computed | 0.5 d | unblocks every rig a model writes, forever |
| 2 | §2.1 `check_printability`, per body per orientation | 3 d | answers the user's own complaint; on the route every printed part takes |
| 3 | §2.3 penetration depth + contact area in `fit_between` | 1 d | makes existing verdicts mean what models already read them as |
| 4 | §2.2 `check_motion` with a conservative-advancement certificate | 4 d | replaces four hand-built rigs with one call, and is *certain* rather than sampled |
| 5 | §2.5 `fit()` — printed clearances from a sourced table | 1 d | six unsourced clearances in one part become one table |
| 6 | §2.4 `check_flex` — strain, force, and layer direction | 5 d | the only honest answer to the user's first objection |
| 7 | §2.7 `OBJECTS` catalogue (coins, cells, cards), with profiles | 2 d | upgrade of NEXT.md queue item 5 |
| 8 | §2.8 print orientation per body, lay-flat export, exploded render | 3 d | makes `stands_on` true for assemblies and 3MF printable as shipped |
| 9 | §2.9 snap / catch / hinge DSL primitives | 4 d | only worth it *after* 6, which is what would make them refuse |

**Build #2 first.** Reasons in §3.

---

## 2. The proposals

### 2.1 `check_printability` — per body, per candidate orientation

**(a) The transcript moment.**

> **USER:** nice BUT / you have overhangs 3d printer would struggle with / consider leveraging multipart nature

The model's reply began *"The overhangs are the lips: each one sticks 1.5 mm straight inward, unsupported."* That is a hand inspection of its own source, not a measurement, and it was made after five earlier claims of "no supports". It had no tool. `docs/PERCEPTION.md` §6 (line 780) marks overhang **❌** and scopes exactly this; `stands_on` and `prints_on` are the two pieces already done.

Worse, the one printability number the reports *do* carry went wrong in a way nobody noticed. Every multi-body report in the second half of the session says of the top plate:

```
"name":"top", "stands_on":{"area_mm2":22.392,"footprint_fraction":0.004,"patches":10,"z_mm":0.1}
```

0.4 % of the footprint on 10 patches is the signature `stands_on` was *built* to raise (PERCEPTION §6: the plate stand's "18 patches, 7 %"). Here it is a false alarm: the body is drawn in its **assembly** orientation, skirt and snap pins hanging down, and prints flipped. The model ignored the line, correctly and for no stated reason. A check that is wrong on every assembly is a check every model learns to skip — the exact failure PERCEPTION §5 records about putting the thin-wall minimum in `evaluate_part`.

**(b) The tool.**

```
check_printability(script, up?: [x,y,z] | "auto", per_body?: bool,
                   overhang_deg = 45, nozzle_mm = 0.4, layer_mm = 0.2,
                   timeout_s?)
```

`up` defaults to each body's declared print orientation (§2.8) and falls back to `+z`. `"auto"` reports the six axis orientations ranked, and says it ranked only those.

Reply, written to be read (rules first, measured numbers, a place for every number):

```
note: an angle is between a face's outward normal and the build plane: 90° is a
wall, 0° is a ceiling. Faces at or below overhang_deg need support or bridge.
Exact on planes, cylinders and cones; sampled on nurbs, within nurbs_bound_mm.

body "top", up = -z (declared)
  bed: 2841 mm2, 1 patch, 45% of the footprint   <- first layer
  overhangs below 45°: 2 faces, 24.1 mm2 in all
    45.0° cone r 2.10 at (-46.1, 0.0, 0.65) x4, 12.1 mm2   at the limit, tag "top"
     0.0° plane facing -z at (0.0, 0.0, 0.10), 12.0 mm2, spans 6.20 mm
           nothing below it for 10.54 mm -> a bridge of 6.20 mm, or support
  support prism below unsupported faces: 61 mm3
  smallest solid feature: 0.90 mm at (36.6, -0.1, 9.5) = 2.3 nozzles
body "cups", up = +z (declared)
  bed: 827 mm2, 1 patch, 16% of the footprint
  overhangs below 45°: none. Every face is a wall or a ceiling of 90°.
  smallest solid feature: 1.00 mm = 2.5 nozzles
body "base", up = -z (declared)   ...
```

Three design commitments in that reply, all of which the project's own rules demand:

1. **Report the geometry, not the verdict** — PERCEPTION §6 already says this. "45.0° … at the limit" is a measurement; "this will print" is an opinion about a machine we know nothing about. A face *exactly at* the threshold is reported, never passed: the final part's snap barb is a 45.000° cone, and a tool that silently passes it hides that the design has zero margin there.
2. **Exact where the surface is analytic.** A plane's angle is one number for the whole face. On a cylinder or a cone the overhang band is bounded by a parallel on the surface, so its area is a closed form. Only NURBS need sampling, and then the reply carries the bound — the same shape of guarantee `measure_wall_thickness`'s `note` already makes.
3. **A bridge is reported as a span, not as a pass.** Cast a ray down from the down-facing face (`perceive.rs` already does exact ray/face intersection) to find what is beneath and how far; report the face's widest span and the drop. Never say "bridgeable" — that is a machine opinion.

Support volume is exact and cheap: extrude the down-facing faces to the bed and `BRepAlgoAPI_Cut` the part out of the prism. One boolean, a real mm³.

**(c) Files and size.** New `crates/parcad-occt/src/printability.rs` (~450 lines), reusing `perceive.rs`'s face sampling and ray cast. `protocol.rs` (+1 struct), `crates/parcad-evaluation/src/lib.rs` (a `printability` block, as `stands_on` is), `crates/parcad-host/src/{service.rs,mcp.rs}` (+ tool), `crates/parcad-cli` for parity, docs. ~3 days.

**Ride it on the route.** NEXT.md queue item 1 already puts a `print_check` (thinnest wall + collisions) on `export_part` and `save_project`. This is the third line of that block, and I would say so in the brief rather than building a second route: `print_check: 1 unsupported span of 6.20 mm on "top"; smallest feature 0.90 mm = 2.3 nozzles`.

**(d) Proof.**
- `eval/cases/overhang-staircase.json` — a closed form: a cone of known half-angle, a plate with a rectangular notch of known span, a cylinder whose overhang band area is `2πr·h·(fraction)` derived by hand in the case's `why`. Plus `eval/cases/snap-pin-barb.json` holding 45.000° and 12.1 mm² for the actual part, so the number cannot drift.
- `eval/field/will-it-print.md` — give the model a part with one 20° roof and one 44° wall in an orientation that is not the drawn one, ask which faces need support and how big the unsupported span is. SOUND only when it calls the tool and quotes the span; the trap is quoting the 44° face as a problem, or reading the assembly orientation. Both arms, both thinking settings.

**(e) Prior art.** PrusaSlicer (`SupportMaterial.cpp`) and CuraEngine both do facet-normal-angle detection, and both are **AGPL-3.0 — read the method, never port a line**; the method itself is one dot product and is not theirs. The better-fitting reference is `[agentscad]` already cited in PERCEPTION §6 (the same deterministic rule, plus radius of gyration as a footprint proxy). OCCT gives everything else under the LGPL path already vendored: `BRepGProp` for exact face areas, `BRepAlgoAPI_Cut` for the support prism, `BRepIntCurveSurface_Inter` for the drop below a face (already wrapped). For `up: "auto"`, check Tweaker-3's heuristic and **its licence** before reading it; six axis orientations ranked by unsupported area is honest without it.

---

### 2.2 `check_motion` — the four hand-built rigs, done once and certainly

**(a) The transcript moment.** The model wrote a verification rig four separate times, each time re-declaring the coin table and re-deriving the layout, then judged it by eye from a wall of `between_bodies` rows:

- full stacks as one named body, 36 pieces (`scripts/13`, `16`, `43`);
- a €2 coin at the cup's analytically derived maximum tilt (`scripts/22`):

  ```js
  const maxTilt = (k) => Math.asin(innerH / Math.hypot(d, t)) - Math.atan2(t, d);
  bodies["tip_" + c.k] = cylinder(d/2, t).rotate("y", maxTilt*180/Math.PI - 0.3).at(...)
  ```

- a coin slid 8 mm out along its exit path, as a body called `leaving`;
- a control body called `bare` placed 40 mm in the air to prove the *absence* of the catch let it through.

That is a kinematic interference study, hand-built, four times, in a language with no word for it. The third rig's reply is 14 `between_bodies` rows of which 2 mattered.

**(b) The tool.**

```
check_motion(script, moving: "body name", along: [{ at:[x,y,z], turn?:["z",deg] }, ...]
             | { slide:[dx,dy,dz], steps? } | { turn:["axis",from,to] },
             against?: ["body", ...], timeout_s?)
```

and a DSL counterpart so a script can carry its own study, `motion(body, { slide: [0, 12, 0] })`, evaluated like a body but never exported.

```
note: a path is checked by advancing, not by sampling: at each pose the least
distance to each other body is measured, and the next step is no longer than
that distance, so no contact between steps is possible. first_contact is exact
to within step_bound_mm. depth_mm is how far the two bodies overlap at the
deepest point, which is what a catch has to be pushed through; mm3 is not.

moving "coin_2e" against "cups", 22 poses, step_bound 0.004 mm
  clear for the first 3.15 mm of travel, least clearance 0.13 mm at (48.1,-28.8,5.8)
  first contact at 3.15 mm, on "cups" at (-25.8, 20.0, 3.2), tag "cup"
  at the end of the path: interfering, depth 0.38 mm, over 1.9 mm2 of contact
  to pass, "cups" must move 0.38 mm at that point
```

`depth_mm` is the number the model actually needed and never had (§2.3). `step_bound_mm` is what makes it a *certificate* rather than a sample — the same standard `measure_wall_thickness` already sets with "certain, whatever the sample count".

**(c) Files and size.** `crates/parcad-occt/src/motion.rs` (~350 lines) around the existing `backend::fit_between`; the advancement bound needs the moving body's maximum point speed along the step, which is `|Δtranslation| + |Δrotation| · R_max` from the body's own bounding sphere — a closed form, five lines. `graph.rs` (`Op::Motion`, `children_of`), `dsl.ts`, `protocol.rs`, `mcp.rs`, a case. ~4 days.

**(d) Proof.**
- `eval/cases/pin-through-slot.json` — a Ø6 pin slid through a 6.4 mm slot with a 0.3 mm shoulder: first contact at a hand-derived distance, depth 0.20 mm at the end. Closed form in the `why`.
- `eval/field/does-the-coin-come-out.md` — a two-body script, a cup and a disc, ask at what travel the disc first touches and by how much it has to spread the arms. The trap is the answer derived from the script's own `mouthW` constant, which is the failure PERCEPTION §3 records verbatim ("a computed one wearing a measurement's clothes").

**(e) Prior art.** Conservative advancement is the standard certificate: Mirtich's *V-Clip* work and Zhang/Lee/Kim's CCD papers; **FCL** (Flexible Collision Library) implements it and is **BSD-3 — portable with attribution**, worth reading even though our distance query is OCCT's `BRepExtrema_DistShapeShape`, which is exact where FCL's is on meshes. Check `BRepExtrema_ShapeProximity` for the contact-patch half. For penetration depth on exact B-reps there is no OCCT primitive; the honest implementation is the deepest point of the common solid from the other body's boundary, which is `BRepAlgoAPI_Common` then a distance query — expensive but exact, and it is the number that matters.

---

### 2.3 `between_bodies` answers with the wrong quantity — a one-day fix

**(a) The transcript moment.** Two of them, both read wrongly and both load-bearing.

*A 2-nanolitre graze read as retention.* `scripts/21`:

```
{"a":"holder","b":"tipped","interference_mm3":0.002,"verdict":"interfering"}
```

and the model's summary: *"All checks pass: full stacks fit, and both flat and tilted €2 coins remain securely blocked by the retention arms."* 0.002 mm³ over a 2.2 mm coin edge is a penetration of order **2 µm** — an order of magnitude inside any printer's dimensional error. It is evidence that the coin is exactly *at* the escape boundary, which is the opposite of "securely blocked". The redesign that followed pushed the numbers to 0.378–1.309 mm³ and was reported as *"every one of the 8 coins now hits the arms"* — still under half a millimetre of overlap on most of them, and still reported as a volume the reader must convert to a depth in their head.

*A single touch point read as a flush stack.* Every three-body report:

```
{"a":"top","b":"cups","clearance_mm":0,"closest_mm":[[45,0,12.32],[45,0,12.32]],"verdict":"touching"}
```

`closest_mm` is one point repeated. The model wrote *"the three touch without overlapping"* as evidence that the stack seats. It cannot be: `touching` is returned identically for two bodies that share 2800 mm² of face and for two that graze at a corner, and here the reported point is at the extreme +X end of the part, which is a corner.

**(b) The change.** Two fields on `FitReport` / `BodyFit`, and a line in the tool description:

```
verdict "interfering": interference_mm3 61.2, depth_mm 0.38 at (-25.8, 20.0, 3.2)
   depth_mm is how far one body reaches into the other at the deepest point:
   what has to move for them to fit. A volume of 0.002 mm3 is a graze, not a fit.
verdict "touching":    contact_mm2 2841.0 over 1 patch, centred (1.5, -0.4, 12.32)
   a touch over one patch of most of a face is a seat; a touch of 0 mm2 at one
   point is two bodies that graze at a corner.
```

**(c) Files.** `crates/parcad-occt/src/backend.rs` `fit_between` (~+80 lines), `protocol.rs`, `crates/parcad-evaluation/src/lib.rs` (`BodyFit`), `mcp.rs` descriptions for `evaluate_part` and `check_fit`, the `eval/cases` that record `between_bodies` (`--update` where the change is intended). ~1 day.

**(d) Proof.** `eval/cases/two-boxes` already holds a body pair; add a case with a 5 mm cube sunk 0.4 mm into a plate — `depth_mm` 0.400 and `interference_mm3` 10.0, both derived by hand — and a second with two plates seated face to face: `contact_mm2` exactly the overlap rectangle. Field case: reuse `does-the-lid-clear.md` with a lid that grazes one corner; SOUND requires "touching at a corner, not seated".

**(e) Prior art.** `BRepExtrema_ShapeProximity` (OCCT 7.x) gives overlapping face sets, which is the contact patch cheaply; `BRepGProp::SurfaceProperties` on the common shell gives its area exactly. Penetration depth as "deepest point of the common solid" is the same shrinking-ball machinery `perceive.rs` already has for inscribed balls — the deepest inscribed ball in the common solid *is* the penetration depth, so this may be ~40 lines of reuse rather than new code.

---

### 2.4 `check_flex` — the only honest answer to "is pushing the thick plastic feasible?"

**(a) The transcript moment.** The user's second message is the whole case:

> **USER:** do you think pushing the thick plastic is feasable?

and the last line of the last answer, five redesigns later:

> **The pin prongs are the fragile part.** Where the barb meets the split, the prong edges are 0.6–0.9 mm thick. They should hold in PETG; in PLA they may snap on the first press. If they're too stiff, lower `barbOver` from 0.35 mm.

Everything mechanical in that paragraph is a guess, and a lever the model did not have is sitting in plain sight: cantilever strain goes as `1/L²`, and the slot that sets `L` is a constant in its own script (`slotL = 6`).

**(b) What is computable, exactly, from geometry the kernel already measures.**

I derived the following by hand from `scripts/44-save_project.js`. It is what the tool would produce, not something ParCAD measured:

- The split pin's prong is a circular segment of a Ø3.5 shaft cut by a 1.0 mm slot, so it spans x ∈ [0.5, 1.75]; area 3.085 mm², centroid at x = 1.020, extreme fibre `c = 0.730` mm.
- Free length: slot top at z = 5.6, barb crest at z = 0.65, so `L = 4.95` mm.
- Required deflection: barb radius 2.10 into a 1.85 bore ⇒ `δ = 0.25` mm each.
- Constant-section cantilever: `ε_max = 3δc / 2L² = 3(0.25)(0.730) / 2(4.95²)` = **1.12 %**.
- And the direction matters more than the number: the pin prints **upright**, so the tensile fibre at the root runs along the build axis — the strain is **90° across the layer interfaces**, the weakest direction an FDM part has.

That is the entire content of "in PLA they may snap on the first press", derived in four lines from numbers the kernel can measure, and it comes with the fix the model could not name: `slotL` 6 → 10 mm takes `L` to 8.95 and the strain to **0.34 %**, a 3.4× margin gained by lengthening a slot.

**(c) The tool, and where it must refuse.**

```
check_flex(script, member: "tag or body", deflect: { by: mm, at?: [x,y,z], direction?: [..] },
           material?: "pla" | "petg" | "abs" | { E_mpa, strain_pct, source })
```

```
note: a strain is exact only for a prismatic member. This tool identifies the
member's neutral axis and section from the built solid and refuses anything it
cannot solve in closed form: a tapered, branched or varying section is refused,
not approximated. Force assumes the modulus you named; strain does not.

member "pin" (4 of them), straight cantilever
  free length      4.95 mm   root at z 5.60, load at z 0.65
  section at root  3.085 mm2, extreme fibre 0.730 mm from the neutral axis
  deflection       0.250 mm  (barb 2.100 into a 1.850 bore, measured)
  strain at root   1.12 %
  force            5.9 N per prong at E = 2000 MPa (pla, across layers, see source)
  build direction  the tensile fibre runs 90° to the layer plane: this is the
                   weakest direction of an FDM part and where it breaks first.
  lever            strain goes as 1/L^2: a free length of 8.95 mm reads 0.34 %.
  not answered     insertion force. It needs a friction coefficient, which for a
                   printed flank is 0.2 to 0.6: at a 45.0° lead-in that is 9 to
                   31 N. Give `friction` to get a number, and say where it came from.
```

The `pla` entry must look like `DEVICES` does — a `source` field that says whose filament, whose printer, and which axis — because FDM anisotropy is 30–60 % and a single number for "PLA" is a lie. The tool reports the strain regardless of material; the material only turns it into a force and a comparison.

**Where it must refuse.** The C-cup arms — *"the arms spring open … flat coins need 2.0 mm of spread for the big coins and up to about 2.7 mm for the 1c"* — are **not** straight cantilevers. They are circular-arc beams of 1.2 mm wall on a 13.8 mm mean radius, and a straight-beam formula applied to them is wrong by a large factor. Two honest outcomes only:

1. implement the circular-arc case properly (Castigliano on the arc; the closed form is in Roark, Table 9.2, circular rings and arches) and report it as exactly as the straight case; or
2. refuse: *"member `cup` is an arc of radius 13.775 over 218°, not a straight cantilever. Its strain is not the cantilever formula. Not solved."*

Anything between those two is the approximation this project refuses. I would build both cases — the arc is the more common one in printed parts (every C-clip, every ring catch) and it is still a closed form.

**(d) Files and size.** `crates/parcad-occt/src/flex.rs` — identify the member's axis and section by cutting it with planes normal to its axis (`BRepAlgoAPI_Section` + `BRepGProp` for area, second moments and centroid: OCCT gives `GProp_GProps::MatrixOfInertia` directly, so `I` and the neutral axis are a kernel call, not integration code). Prismaticity is checked by comparing sections along the axis and refusing on a difference. ~5 days including the material table, which is the slow part because it is research, not code.

**(e) Proof.**
- `eval/cases/cantilever-strain.json` — a 20 × 4 × 2 mm rectangular beam deflected 1 mm: `ε = 1.5·δ·h/L² = 1.5(1)(2)/400 = 0.75 %` exactly, and `I = bh³/12 = 2.667 mm⁴`, `P = 3EIδ/L³`. Both hand-derived in the `why`.
- `eval/cases/split-ring-arc.json` — the arc case against Roark's closed form.
- `eval/field/will-the-snap-hold.md` — give the model a snap pin and ask for the strain at the root and what to change to halve it. SOUND requires calling the tool, quoting the strain, and naming the free length (not the barb) as the lever; the trap is answering "use PETG".

**(f) Prior art and licence.** The formulas are engineering facts in the public domain; the *documents* are copyrighted, so implement from the formula and cite: BASF *Snap-Fit Design Manual* (1998), Bayer/Covestro *Snap-Fit Joint Design* (the `W = P(μ + tan α)/(1 − μ tan α)` insertion-force relation and the `ε_perm ≈ 0.5·ε_yield` rule for a one-time assembly), DuPont *Design Handbook for Engineering Polymers*, Roark's *Formulas for Stress and Strain* Table 9.2 for arcs. For the material table: filament manufacturers' own TDS (Prusament, Bambu, Polymaker each publish tensile modulus and elongation) plus published FDM anisotropy studies for the Z-axis knockdown — each row labelled with its source and its axis, the way `DEVICES.source` already is. Do not source it from MatWeb (proprietary) or from a blog.

---

### 2.5 `fit()` — six unsourced clearances in one part

**(a) The transcript moment.** `scripts/44` carries, in twenty lines:

```js
const coinGap = 0.3;      // per side, cup over coin
const heightGap = 0.4;
const skirtFit = 0.3;     // cups inside the skirt
const slotGap = 0.5;      // per side, coin through its edge slot
const pinHoleFit = 0.1;   // per side, pin in the base plate
const pinPassFit = 0.15;  // per side, barb through the cups' posts
```

Six different printed-fit allowances, all invented, none sourced, and the one that matters most (`pinHoleFit = 0.1`) silently determines the snap's deflection and therefore its strain. `METRIC_FASTENERS` exists precisely so a machined hole is never a literal ("drill and clearance sizes — never a literal in a part", CLAUDE.md). A printed fit is the same problem and has no table.

**(b) The DSL.**

```ts
/** A printed clearance per side, mm, for a fit a printer can actually make. */
export function fit(kind: "press" | "slip" | "free" | "hinge", nominal_mm: number,
                    options?: { process?: "fdm" | "sla" }): number
```

`fit("slip", 3.5)` → 0.15. The doc comment says where each number came from and that it is per side; the values are per-process and carry a `source` in the table the way `DEVICES` does. An unknown kind is refused, never guessed — same as `fastener()`'s existing refusal, which is the model to copy verbatim.

**(c) Files.** `app/src/dsl.ts` (~60 lines with the table and the doc comment), `docs.rs` picks it up automatically. ~1 day including sourcing.

**(d) Proof.** `eval/cases/` cannot hold a fit — it is a constant, and a case would just re-read the table. The real proof is a field case, `eval/field/what-gap-for-a-printed-pin.md`: ask for a pin and a bore that slide; SOUND when the script calls `fit()` rather than writing 0.1.

**(e) Prior art.** There is no ISO for FDM fits. The honest sources are measured: published test prints (the "tolerance test" family on Printables / Thingiverse, mostly CC-BY), Prusa's and Bambu's own dimensional-accuracy documentation, and — best — a ParCAD test coupon printed once on the owner's own machine, which is one afternoon and makes the table *ours* and measured, exactly like the DEVICES radii note in NEXT.md ("A caliper on each machine settles them for everyone").

---

### 2.6 `note()` — the model built a data channel out of geometry

**(a) The transcript moment.** The single most damning line in 45 scripts. `scripts/22-evaluate_part.js`, last line before the return:

```js
bodies.note = box(1, 1, 1).at(0, 200 + cups.map((c) =>
  `${c.k}:${mouthW(c.k).toFixed(2)}/${tiltedWidth(c.k).toFixed(2)}`).join(" ").length, 0);
```

It built a 1 mm cube, placed it 200 mm plus **the character count of a string it wanted to read**, and got back:

```
{"a":"holder","b":"note","clearance_mm":301.331,"closest_mm":[[-25.418,21.201,0],[-0.5,321.5,0]]}
```

From which it could recover exactly one integer: 122, the length of the string. The model wanted to see the mouth widths its own script had computed, and the only channel out of the sandbox was the position of a body.

A script computes real engineering — `maxTilt`, `tiltedWidth`, `mouthW`, `bevel`, `innerH`, the pin positions — and none of it is visible in any reply. The model must either re-derive it in its head (and does, wrongly, at least once) or smuggle it.

**(b) The DSL.**

```ts
/** Report a value from the script. It appears in the reply's `notes`, and
 *  changes nothing about the part. */
export function note(label: string, value: number | string | number[]): void
```

Reply:

```
notes (from the script, not measured):
  mouth 2€   23.75 mm   tilted width 24.42
  mouth 1c   13.85 mm   tilted width 15.05
  bevel       1.42 mm
```

Capped (say 40 notes, 2000 characters, truncated with a count) so it cannot become a channel for dumping the script back.

**(c) Files.** `app/src/dsl.ts` (+15 lines), `crates/parcad-host/src/script.rs` (collect from the sandbox — the QuickJS boundary already returns a graph, this is one more field), `crates/parcad-evaluation/src/lib.rs` (a `notes` vector), `mcp.rs`. Half a day.

**Honesty guard, and it matters.** `notes` are *requested* values, not measured ones — the exact thing CLAUDE.md's "report measured values, not requested ones" warns about, and PERCEPTION §3 records a model quoting a derived number "at 99 % confidence". So the reply must segregate them under the heading above. With that line it is a debugging aid; without it, it is a new way to launder a guess.

**(d) Proof.** A `docs.rs` test that the example runs; a field case is probably not needed, but `eval/field/where-is-the-sliver.md` should be re-run to confirm notes do not become the thing trials quote instead of a measurement — that is the one risk worth measuring.

---

### 2.7 A measured `OBJECTS` catalogue — upgrade of NEXT.md queue item 5

**(a) The transcript moment.** Ten lines, retyped from memory into *every one* of the 45 scripts:

```js
const COINS = { "2€": { d: 25.75, t: 2.2 }, "1€": { d: 23.25, t: 2.33 }, ... };
```

I checked them against the ECB's published dimensions: **they are all correct.** That makes the point sharper, not weaker. Nothing in the session could have told the model or the user whether they were right, the numbers carry no provenance, and the next part — SD cards, 18650 cells, a USB-C plug — will be the same ten lines and may not be lucky. This is the `DEVICES` argument verbatim, from that table's own comment: *"The V holder for a 16" MacBook Pro shipped with two radii guessed in a script; the guess now lives here, labelled as one."*

There is a real mechanical error hiding in it too. The **20 c coin is not a circle** — it is the "Spanish flower", seven rounded lobes, 22.25 mm across the lobes. A cup bored to Ø22.85 holds it with up to 0.6 mm of extra slop between lobes, and more importantly the *mouth squeeze* check — the whole retention argument — depends on which way round the coin is presented. The model's worst-case tilt study is exact for a cylinder and inapplicable to the one coin in eight that is not one. A catalogue that carries only `d` and `t` cannot express this; a catalogue that carries a **profile** can.

**(b) The DSL.** Extend the existing `DeviceBody` / `device()` pattern rather than inventing a parallel one:

```ts
export const OBJECTS: Record<string, ObjectBody>;   // "coin/eur-2", "cell/18650", "card/sd", "insert/m3-heatset"
export function object(name: string, o?: { clearance?: number }): Shape;
export function objectsIn(group: string): string[];  // objectsIn("coin/eur") -> the eight
```

with `ObjectBody` carrying an optional `profile: SectionEntry[]` — which the DSL's section grammar already expresses exactly, arcs and all — so the 20 c is a drawn flower and not a circle, and `object("coin/eur-20c")` is an extrude of it. Every entry keeps `source` and the honesty rules of `DEVICES`: published sizes where a mint or a standard publishes them, *labelled* estimates where they do not, no entry invented.

Heat-set inserts belong here and are the highest-value row for printed parts: `insert/m3-heatset`, with its boss bore and boss OD from the maker's published installation drawing (Ruthex, CNC Kitchen), is the printed-parts equivalent of `holeFor` and would have removed the whole M2 screw-boss detour in `scripts/26-28`.

**(c) Files.** `app/src/dsl.ts` (~200 lines of table plus two functions), a case per family in `eval/cases/` (`device-body.json` is the precedent). ~2 days, mostly sourcing.

**(d) Proof.** `eval/cases/object-coin-eur-20c.json` — the flower profile's area by shoelace-plus-arc-segments, times thickness, against the built volume; `eval/cases/object-cell-18650.json` — trivially a cylinder, there to pin the dimensions. Field: `eval/field/hold-this-thing.md` asks for a holder for a named object; SOUND requires reaching the catalogue rather than typing a dimension.

**Say it is an upgrade.** NEXT.md queue item 5 already says "a measured parts library — fasteners, bearings, boards, devices". This adds: the *profile* field (a non-circular object cannot be a box or a cylinder), grouped names so a set can be iterated, and inserts as the first-class printed-parts row.

---

### 2.8 Print orientation per body, lay-flat export, exploded render

**(a) The transcript moment.** Three of them.

- The `stands_on` false alarm in §2.1 — `footprint_fraction: 0.004, patches: 10` on every report of the finished part, because a body drawn in assembly is not a body as printed.
- *"Top plate: printed face-down … Cups (orange): printed standing … Base plate: flat"* — three different print orientations, stated in prose, recorded nowhere a tool can read, and therefore unchecked by anything.
- The user only ever saw the three parts **assembled**. The final render (`cdbceb8da780d002`) shows a stack; nothing in the session ever showed the three bodies apart, and the model's description of how they go together was prose against a picture in which the cups are barely visible through the windows.

**(b) The pieces.**

```ts
.printedUp(dir: "x" | "-x" | "y" | "-y" | "z" | "-z" | Vec3)   // per body, like .material()
```

Recorded on the graph next to `Material` (`graph.rs` line 2396, `body_materials` line 2551 — the exact same mechanism, read off the graph per body, never followed through the kernel). Then:

- `check_printability` (§2.1) defaults to it, and `stands_on` is reported *in that orientation* — the false alarm disappears and the number becomes true.
- `export_part` with `layout: "print"` writes each body already rotated. **3MF expresses this natively**: a `<build><item transform="…">` carries a matrix per object, so nothing has to be baked into the mesh and the slicer opens the file with three parts lying correctly. This is a handful of lines in the writer.
- `evaluate_part` with `exploded: 12` draws the bodies pushed apart along the stack axis. It is a *render* option, like `section` — the part and every measurement are of the assembled part, exactly as `section` already promises.

**(c) Files.** `graph.rs`, `dsl.ts`, the 3MF writer in `crates/parcad-host/src/service.rs`, `render.rs` for the exploded camera, `app/src/viewport.ts` for parity in the window. ~3 days.

**(d) Proof.** `eval/cases/three-plate-stack.json` pinning each body's `stands_on` in its declared orientation (all three near 100 % of their own footprint, 1 patch) against the assembled numbers. Field: `eval/field/lay-it-out-to-print.md` — SOUND when the reply says each body's bed face and the export carries the transform.

**(e) Prior art and overlap.** NEXT.md's "Integrations before launch" already queues **colour** per body in 3MF (`basematerials`, waiting on materials reaching export) — this is the *transform* half of the same writer, and the brief should say so and land them together. The 3MF Core Specification's build-item transform is the mechanism (3MF Consortium; check `lib3mf`'s licence if any code is borrowed). Slicers' auto-arrange is not needed: one declared orientation per body is more honest than a solver, and matches `prints_on`'s existing conservatism ("a diagonal placement that would fit is reported as not fitting").

---

### 2.9 Snap, catch, hinge, press fit — DSL primitives, but only after `check_flex`

**(a) The transcript moment.** The split pin is nine dense lines of cones and a slot box, hand-derived:

```js
const pin = union(
  cylinder(pinR, zTop - barbTop + 0.01).at(0, 0, (zTop + barbTop) / 2),
  cone(pinR + barbOver, pinR, barbTop - barbMid).at(...),
  cone(pinR + barbOver - (barbMid - tipZ), pinR + barbOver, barbMid - tipZ).at(...),
).cut(box(slotW, 2 * (pinR + barbOver) + 1, slotL).at(...));
```

with the claim *"The barb is angled at 45° on both faces. That lets the pins print upright with no overhang"* — true, and exactly at the limit, and never checked. Separately, the model hand-wrote `flaredOpening(w, reach, h, r)` — a lead-in radius cut into an opening — after **four** consecutive fillet refusals, and left a comment explaining why: *"The round is cut in with the opening because the kernel will not fillet an edge that dies into a face."* Every hand-reachable opening in every printed part wants that, and it cost four rounds.

**(b) The primitives, with what each refuses.**

```ts
snapPin(o: { shaft: number, length: number, barb: number, lead?: number,
             catch?: number, split?: "two" | "four" | "none", free: number }): Shape
cantileverCatch(o: { length, width, thickness, barb, lead?, catch? }): Shape
opening(o: { width, reach, height, lead: number }): Shape   // the flaredOpening above
livingHinge(o: { length, thickness, gap }): Shape
```

Each **refuses rather than approximating**, and each refusal names the number:

- `snapPin` refuses a lead angle steeper than the caller's declared overhang limit, saying which: *"a 30° lead prints as a 30° overhang; 45° or shallower, or declare the pin's build direction."*
- `snapPin` and `cantileverCatch` refuse when the implied root strain exceeds a stated limit, **by calling `check_flex`'s own solver** — which is why this item comes after §2.4, and why building it before would produce four more shapes nobody can check.
- `livingHinge` refuses below one extrusion width and names the nozzle.
- `opening` refuses a lead radius larger than half the width.

**(c) Files.** `app/src/dsl.ts` (~250 lines), sharing the strain solver over the same closed forms. ~4 days after §2.4.

**(d) Proof.** `eval/cases/snap-pin.json` pinning volume, the barb's cone angle and the prong section; `eval/cases/refuse-a-steep-barb.json` in the refusal family alongside the existing `refuse-*` cases. Field: `eval/field/make-it-click-together.md` — ask for two plates that snap; SOUND requires `snapPin` and quoting the strain, and the trap is nine lines of cones.

---

## 3. What I would build first, and why

**`check_printability` (§2.1), with `note()` (§2.6) as a half-day rider in the same session.**

1. **The user asked for it in their own words** — "you have overhangs 3d printer would struggle with" — and it was the only objection of the three that the model answered by restructuring the whole part rather than by measuring.
2. **It is on the route every printed part already takes.** NEXT.md queue item 1 is building `print_check` into `export_part` and `save_project`; overhang is the third line of that block and costs a fraction of the whole thing if it lands with it, and a whole extra session if it does not.
3. **The data is already in the kernel.** Exact face areas and normals (`Shape_faces_json` → `FaceSummary`), exact ray casts (`perceive.rs`), exact booleans for the support prism. There is no new mathematics; the work is the per-orientation semantics and the wording.
4. **It repairs a check that is currently *wrong*, not merely missing.** `stands_on` reads 0.4 % / 10 patches on every assembly, and a model that learns to ignore one printability line will ignore the next one too. That is this project's own recorded failure mode (PERCEPTION §5, on why the thin-wall minimum is kept out of `evaluate_part`).
5. **It is the check with the shortest path from a number to a script edit.** "0° face, 6.20 mm span, nothing below it for 10.54 mm" is directly actionable; "1.12 % strain" (§2.4) needs a material argument around it before a model knows what to do.

`note()` rides along because it costs half a day, because the model demonstrably wanted it badly enough to encode a string length into a body's Y coordinate, and because every rig written to test a printability fix will want it.

---

## 4. What cannot be made honest, and should be refused

- **Insertion force as a single number.** `W = P(μ + tan α)/(1 − μ tan α)` is exact; `μ` for a printed flank against another printed flank is 0.2–0.6 depending on layer direction, surface finish and orientation. On the final part's 45° lead that is 9–31 N, a 3.4× spread — which still answers the user's question ("a thumb does 10–30 N comfortably"), but only as a range with its assumption named. Report `P`, report `α`, report the range, take `μ` as an argument, and never print one number.
- **"Will it survive N cycles."** Fatigue in FDM parts is dominated by layer adhesion and void fraction, neither of which is in the geometry. Refuse.
- **"PLA or PETG."** ParCAD can honestly say: the strain at the root is 1.12 %, the tensile fibre runs 90° to the layer plane, and here is a table of published elongation figures with their sources. It cannot honestly say which filament to buy, because the answer depends on the user's printer, their filament's moisture history and their layer height. Report the geometry and the table; let the caller choose.
- **Strain in a non-prismatic member.** Refuse. An arc is a closed form and should be solved; a tapered branching arm is not, and applying a cantilever formula to the C-cups would be exactly the "thread made of stacked tori" that DSL_GAPS calls the silent approximation this project refuses.
- **"Bridgeable."** A 6.2 mm span bridges on most printers and not on all. Report the span and the drop; never the verdict.
- **A collision study by sampling.** If §2.2 ships without the advancement bound it will miss a contact between poses and report "clear" for a part that jams. Either the certificate ships or the tool says `sampled at N poses, a contact between them is possible` in the reply itself.

---

## 5. Two smaller things worth writing down

- **The overhang claim and the barb.** By my hand derivation from `scripts/44`, the final part is probably support-free in the three stated orientations, and its snap barb is a **45.000°** cone — at the threshold with zero margin — while both plates carry a 0.5 mm `edgeRound` on the face that becomes the bed, which shrinks the first layer. None of that was measured, in either direction. The design may well be fine; the point is that "fine" and "at the limit" were indistinguishable to everyone in the room.
- **Four fillet refusals in a row, all the same cause.** `1.5 mm → 0.52`, `0.5 → 0.25`, `0.5 → 0.25`, `0.4 → 0.12`, each naming a measured largest buildable radius, each honest, and the model eventually gave up and cut the round into the cutter by hand. The refusals did their job; what is missing is the op that makes the fix a one-liner. `opening({ lead })` in §2.9 is that op for the hand-reachable case, and it is worth noting that a *finish applied after a cut* is exactly DSL_GAPS §8's "a finish is a later op, not part of the cutter" — this session is a second, independent instance of that gap, in a different kind of part.

---

# Appendix B — language / tool-surface lens

# What ParCAD's language, tools and errors cost one coin-holder session

Read from the LANGUAGE / TOOL-SURFACE / ERROR-MESSAGE lens only. Every item is
anchored to a moment in the transcript and to a line of the repository. Nothing
here duplicates docs/NEXT.md's queue; where an item touches it, the overlap is
stated.

## The session in numbers

| | |
|---|---|
| tool calls to ParCAD | 78 (33 `evaluate_part`, 13 `read_docs`, 11 `open_project`, 6 `save_project`, 5 `measure_wall_thickness`, 1 `probe_part`, 2 `list_projects`, 3 `read_project`) |
| script bytes sent **to** the host | 240,779 across 45 calls (~60 k tokens), largest 11,253 |
| the same work sent as patches | 49,375 bytes — **79 % less** (difflib over the 45 files, cheapest of the previous six as base) |
| round trips lost to language/tool/error problems | **11** — 1 reserved word, 1 selector grammar, 1 `rotate` arity, 1 section field name, 2 numeric strings, 1 material colour, 4 fillet retries — plus 1 `read_docs` chasing a wrong hypothesis |
| bytes burned on those 11 | ~62 KB outbound; the four fillet retries alone are 39,888 |
| tools never called | `set_script`, `check_selector`, `inspect_treatment_target`, `list_entities`, `check_fit`, `export_part`, `get_session`, `list_snapshots` |
| DSL features never used | `.expect({count})` in **0 of 45** scripts; `chamfer` 0, `dihedral` 0, `longerThan` 0, `hull`/`line2d`/`inset`/`repeat`/`grid` 0 |
| times the model said a check was not re-run | 2 |

Two structural facts sit behind most of the list: **every call carries the whole
script** (there is no way to evaluate what is saved and no way to send an edit),
and **every check lives in a throwaway script** (nothing lets a part carry its
own assertions). Those are B1 and B2 at the end.

---

# Part 1 — the eleven lost round trips, and what else I found

## L1. `const clearance = 0.6` — "invalid redefinition of parameter name"

### (a) The moment

First build of the session:

```
### TOOL_RESULT
Error: the script did not parse at line 18:
invalid redefinition of parameter name
  18 | const clearance = 0.6; // bore over coin diameter
```

The model's next action was `read_docs {"section": "polar"}` — it did not know
what had gone wrong, so it went looking at an unrelated function — then rewrote
and resent 3,237 bytes. One round trip plus one documentation call.

**The damning part**: it had already read the rule. Its first `read_docs` reply,
eleven calls earlier, contains

> "Every name below is a reserved word in a script: a variable called `box`
> hides the primitive."

and the contents listing right under it names `clearance` in "Functions (1 of
2)". The rule was in context, the name was in context, and the collision still
happened. **This hazard cannot be documented away** — 118 names is more than a
model will diff its locals against. It has to be caught at the point of failure.

### (b) Root cause

`crates/parcad-host/src/script.rs:104-107`:

```js
  let fn;
  try {
    fn = new Function(...names, source);
  } catch (e) {
    return failed("the script did not parse", e);
  }
```

`failed` passes QuickJS's own words through. QuickJS says "invalid redefinition
of parameter name" and names nothing; JavaScriptCore in the app says something
else; V8 in a browser tab a third thing. The runner holds `names` (every DSL
export) and the source at that exact point and uses neither.

docs/DSL_GAPS.md §7 already specifies the fix and it is not built:

> **Fix:** the places that compile a script (`tools/run.ts`,
> `app/src/engine.ts`, `crates/parcad-host/src/script.rs`) should catch the
> redeclaration and name the builtin that was shadowed. "`hole` is a parcad
> builtin — rename your local" is a one-line fix for the reader; "Cannot declare
> a const variable twice" is not.

### (c) The fix

Do not guess from a regex alone — **prove it**, the way
`crates/parcad-core/src/envelope.rs:220-231` already proves which field failed
("the field whose removal changes it is the one that failed"). On the failure
path only, recompile without each suspect name; the one whose removal parses is
the collision. A false positive then cannot happen, and the happy path pays
nothing.

One implementation, three call sites. Export it from `app/src/dsl.ts` under the
`__parcad` prefix that exists (dsl.ts:185, `__parcadTreatmentSource`) precisely
so a helper does not itself become a reserved word:

```ts
/** @internal Which builtin a script redeclared, proved by recompiling without it. */
export function __parcadShadowedBuiltin(source: string, names: string[]): string | undefined {
  const declared = new Set(
    [...source.matchAll(/(?:^|[;{}\n])\s*(?:const|let|var|function|class)\s+([A-Za-z_$][\w$]*)/g)].map((m) => m[1]),
  );
  for (const name of names) {
    if (!declared.has(name)) continue;
    try {
      new Function(...names.filter((n) => n !== name), source);
      return name;
    } catch {}
  }
  return undefined;
}
```

Called from `script.rs`'s `RUNNER`, `app/src/engine.ts:174-180` and
`tools/run.ts:27-32`. The new reply:

```
Error: the script did not parse at line 18:
`clearance` is one of parcad's 118 builtins, so it is already a name in every
script and cannot be redeclared. Rename the variable — `coinClearance` — or drop
the line and call `clearance("M3")`, which is what the builtin does.
  18 | const clearance = 0.6; // bore over coin diameter
```

It names the fix, names the word, gives the replacement, and says what the
builtin is for so a model can tell a collision from a mistake.

### (d) Size

Half a day: one exported function, three call sites, one test.

### (e) How to measure

A unit test in `script.rs`, `a_redeclared_builtin_is_named_with_its_fix`. Then a
field case, because reading it is the separate fact:

```
---
tool: evaluate_part.refusal
reach: evaluate_part
verdict: RENAMED\s*[=:]?\s*[A-Za-z_$][\w$]*
trap: RENAMED\s*[=:]?\s*(clearance|none)\b
why: Every DSL export is a parameter of every script, so a local called `clearance`
  is a SyntaxError from an engine that names no identifier. The rule is in the first
  paragraph of read_docs `dsl` and was read, in context, by the session that then hit
  it — so this case measures the error message, not the documentation. A trial that
  answers `clearance` renamed nothing and read the message as a mystery.
---
```

with a prompt handing over a 20-line script declaring `const clearance = 0.6`,
asking for it to build and for `RENAMED = <the new name>`.

---

## L2. `">Z and not |Z"` — `not` is not in the grammar, and the error does not say what is

### (a) The moment

```
### TOOL_RESULT
Error: the script threw at line 88:
invalid edge-selector term "not |Z"; expected >X, <Y, or |Z (joined with `and`)
  88 | .fillet(1.0, ">Z and not |Z")
```

The script held **two** such selectors (line 88 and line 89,
`.fillet(0.6, "<Z and not |Z")`); only the first was reported. 3,346 bytes
resent.

### (b) Root cause

Two identical parsers by design: `crates/parcad-core/src/selectors.rs:683` and
`app/src/selectors.ts:116`, held together by `eval/selectors.json`, which
records this exact message (case `'>Z or >Y'`). The message names the three term
spellings and stops. It does not mention that the *query form* exists, and the
query form is where every term the model wanted lives — `dihedral`, `parallel`,
`longerThan`, `on`. `app/src/dsl.ts:225` (`assertEdgeSelector`) rethrows it
verbatim, so the caller sees a message about a small grammar with no pointer to
the larger language beside it.

`check_selector` exists, costs no geometry, and was never called all session. An
error is the only place a model is reliably reading at the moment it needs that
tool.

### (c) The fix — two parts

**Part 1, the message.** In both parsers, re-recorded in `eval/selectors.json`:

```
invalid edge-selector term "not |Z"; the compact form is >X, <Y or |Z joined
with ` and `, and has no not, or, or brackets. `not` is not in it: say what you
want instead, in the query form — .fillet(1.0, { at: { z: "max" }, dihedral:
"convex" }), which also has parallel, longerThan, on and between. check_selector
parses one without building anything.
```

The `not`/`or` sentence is added only when the term contains those words —
exactly the mechanism `queryShapeError` already uses for key synonyms
(`app/src/selectors.ts:193`, where `tag → on` and `length → longerThan`). The
span is unchanged; the span is what the editor underlines.

**Part 2, should `not` exist?** Yes, in the **query form only**, not in the
compact string. The compact form is a conjunction of extrema and directions;
`not >Z` is close to meaningless ("not the furthest") while `not |Z` is a real,
common instruction. Negation belongs where every term is already a predicate
over the same candidate set:

```ts
export interface EdgeQuery {
  // …
  /** Edges this sub-query matches are removed from the rest of the match. */
  not?: EdgeQuery;
}
```

Doc comment, written for a model:

> **`not`** — every edge the rest of this query matches, except those the
> sub-query matches.
>
> - The sub-query is resolved over the same edges, before `at` picks extrema, so
>   `at` is measured among what survives.
> - One level: a `not` inside a `not` is refused.
> - Prefer a positive term where one exists; `{ dihedral: "convex" }` is clearer
>   than `{ not: { dihedral: "concave" } }`.
>
> `@example` `box(40, 20, 10).fillet(1, { dihedral: "convex", not: { parallel: "z" } })`
> — breaks every outside edge except the four upright ones.

This needs an `EDGE_QUERY_KEYS` entry (`app/src/selectors.ts:146`), a
`GRAPH_FEATURES` stamp (`dsl.ts:3703`) and an `envelope::FEATURES` id
(`crates/parcad-core/src/envelope.rs:34`): hosts through 0.0.9 silently drop an
unknown query key, which would fillet the wrong edges rather than refuse.
docs/SELECTORS.md §3 and NEXT.md carry `any` (disjunction); `not` is in neither,
so this extends that proposal and the two are one session's work together.

### (d) Size

Message: two hours including the corpus re-record. `not`: two days.

### (e) How to measure

Add `'>Z and not |Z'` and `'|Z or >X'` to `eval/selectors.json` — both parsers
assert against it, so a message changed in one language fails in the other. For
reading it, extend `eval/field/which-selector-holds.md`, which exists for
exactly this, with a fourth selector and a verdict that must name the query form.

---

## L3. `.rotate(0, 0, 45)` — the DSL accepted it and the kernel blamed the host's version

### (a) The moment

```
### TOOL_RESULT
Error: node 15 (rotate), field "axis": invalid type: integer `0`, expected struct
V3. If a newer parcad wrote the part, this host (parcad 0.0.9) is too old for it:
update it (brew upgrade parcad, or the latest release), or point the client at a
newer host
```

The model spent a `read_docs {"section": "Shape.rotate"}`, fixed it to
`.rotate("z", 45 + a)` and resent 3,324 bytes. It was misled enough to report it
to the user at the end of the turn:

> "One thing about ParCAD itself came up. Writing `.rotate(0, 0, 45)` failed
> with a message telling me to upgrade the host, when the real mistake was the
> argument form (it should be `rotate("z", 45)`). The DSL should reject that call
> itself and name the correct form."

A false bug report generated by a misleading error — the worst kind of cost,
because it also spends the user's attention.

### (b) Root cause, two of them

**One**: `app/src/dsl.ts:626-638` validates nothing.

```ts
  rotate(axis: Vec3 | "x" | "y" | "z", degrees: number): Shape {
    const a: Vec3 =
      axis === "x" ? { x: 1, y: 0, z: 0 }
      : axis === "y" ? { x: 0, y: 1, z: 0 }
      : axis === "z" ? { x: 0, y: 0, z: 1 }
      : axis;                                    // ← 0 lands here, unchecked
    return new Shape(([child]) => ({ op: "rotate", child, axis: a, degrees }), [this]);
  }
```

`axis` is `0`, `degrees` is `0`, and the third argument `45` is silently
discarded. This is the OpenSCAD/Fusion form (`rotate([0,0,45])`), so it is the
single most likely wrong call on this surface.

**Two**: `crates/parcad-core/src/envelope.rs:226-236` appends `if_newer()` to
*every* explanation, including a plain type error in a known field of a known op
on a graph this host's own bundled DSL produced microseconds earlier. A newer
writer is a real explanation for an unknown op, an unknown field or an unread
`requires` entry. It is never the explanation for "a field that has always been
a V3 holds a number".

### (c) The fix

**In `dsl.ts`**, reject at the call:

```ts
  rotate(axis: Vec3 | "x" | "y" | "z", degrees: number, ...rest: unknown[]): Shape {
    const a = AXES[axis as string] ?? axis;
    if (rest.length || !isVec3(a) || !Number.isFinite(degrees)) {
      throw new Error(
        `rotate takes one axis and one angle: .rotate("z", 45), or .rotate({ x: 0, y: 1, z: 1 }, 30) ` +
        `for a diagonal axis. Got rotate(${[axis, degrees, ...rest].map(describeValue).join(", ")}). ` +
        `Three angles is the OpenSCAD form; here it is three calls: .rotate("x", a).rotate("y", b).rotate("z", c).`,
      );
    }
    // …
  }
```

The same guard belongs on `mirror` (dsl.ts:657) and `scale` (dsl.ts:675), which
have the identical `: axis` fall-through, and on `translate`/`at` for non-finite
numbers — `NaN` in a translate today produces a graph whose serde error names a
field and not the call.

**In `envelope.rs`**, scope the boilerplate. `if_newer()` stays on
`check_requires`, on the unknown-top-level-key branch (envelope.rs:120-128), on
`unknown variant` (envelope.rs:200-206) and on the unread-field branch
(envelope.rs:172-180). It comes off `explain`'s type-error paths, which say
instead:

```
node 15 (rotate), field "axis": expected a point { "x": 0, "y": 0, "z": 1 }, got
the number 0. The DSL writes this field from `.rotate(axis, degrees)`, so the
call that made node 15 was given the wrong argument.
```

Rendering serde's `expected struct V3` as `a point { "x":…, "y":…, "z":… }` is a
three-line match and is worth it alone: `V3` is an internal type name that
appears in no document a caller can read.

### (d) Size

Half a day for both.

### (e) How to measure

`dsl.test.ts`: `rotate_refuses_three_angles_and_names_the_form`. A Rust test in
`envelope.rs` asserting a V3 type error does **not** contain "too old". Extend
`mcp.rs`'s `a_refusal_carries_what_a_program_can_act_on` (mcp.rs:1909) with the
new message shape.

---

## L4. `section: {plane: "x", offset: 0}` — one wrong field named, one silently ignored

### (a) The moment

```
### TOOL_USE mcp__ParCAD__evaluate_part
{"script": "…", "views": ["iso"], "section": {"plane": "x", "offset": 0}}

### TOOL_RESULT
failed to deserialize parameters: missing field `axis`
```

The model corrected `plane → axis` and called again:

```
{"views": ["iso"], "section": {"axis": "x", "offset": -38}}
```

**That call succeeded, and `offset` was silently dropped.** The field is
`at_mm`. The model asked for a cut at x = −38, got the default cut through the
middle of the part, and reasoned about coin fit from the resulting picture. The
round trip is the small half; a measurement silently taken somewhere other than
where it was asked for is the half that matters, and it is exactly the failure
class docs/GOTCHAS.md exists for.

### (b) Root cause

`crates/parcad-host/src/mcp.rs:247-264`: `SectionRequest` has no
`deny_unknown_fields`, so serde drops `plane` and `offset` and reports only the
missing required field. No request struct in `mcp.rs` has it — 18 of them. The
generic wording comes from rmcp
(`rmcp-3.1.0/src/handler/server/tool.rs:206`), which passes serde's text through.

The repo already solved this one layer down: `queryShapeError`
(`app/src/selectors.ts:170`) refuses an unknown query key *and* suggests the
right one from a `SYNONYMS` table, and `envelope::check_node`
(envelope.rs:163-183) refuses a graph field the host would silently drop,
explicitly because "building it would silently leave out whatever the field asks
for". The MCP argument layer is the one place that rule is not applied.

### (c) The fix, in two tiers

**Tier 1, today**: `#[serde(deny_unknown_fields)]` on every `*Request` struct in
`mcp.rs`. The reply becomes serde's own

```
failed to deserialize parameters: unknown field `offset`, expected one of `axis`, `at_mm`, `keep`
```

which is correct, names the vocabulary, and — the point — **fires on the second
call too**.

**Tier 2, a synonym table**, because `plane` and `offset` are what the rest of
CAD calls these. Wrap the parameters rather than changing 18 call sites:

```rust
/// A tool's arguments, refused in words a model can act on: the field it does
/// not have, the field it meant, and the shape the whole argument takes.
pub struct Args<P>(pub P);

impl<'de, P: DeserializeOwned + JsonSchema> Deserialize<'de> for Args<P> { /* … */ }
```

`Args<P>` deserializes to a `Value`, walks it against `P`'s schema, and on a
mismatch raises `D::Error::custom` with the explained text; schema generation
forwards to `P`, so `tools/list` is unchanged. Each tool then takes
`Parameters<Args<EvaluateRequest>>`. The table lives beside it:

```rust
const SYNONYMS: &[(&str, &str)] = &[
    ("section.plane", "axis"), ("section.offset", "at_mm"), ("section.at", "at_mm"),
    ("section.side", "keep"), ("threshold", "threshold_mm"), ("timeout", "timeout_s"),
    ("size", "image_size"), ("view", "views"), ("project", "name"), ("path", "name"),
];
```

and the message reads:

```
evaluate_part: `section` has no field "offset" — write "at_mm" instead. A section
is { "axis": "x", "at_mm": -38, "keep": "below" }; "at_mm" omitted cuts through
the middle of the part.
```

**Rename nothing.** `at_mm` is what the reply already calls it ("`section` says
which plane was actually cut — `at_mm` and `keep` resolved"), and consistency
between request and reply is worth more than matching Fusion's vocabulary, which
the synonym table absorbs anyway.

### (d) Size

Tier 1: one hour, mechanical. Tier 2: a day, one new type and a table.

### (e) How to measure

A Rust test per tier: `an_unknown_argument_is_refused_by_name`. And a field case,
because reading the correction is separate from producing it:

```
---
tool: evaluate_part.section
reach: evaluate_part
verdict: CUT AT\s*[=:]?\s*-?38(\.0+)?\b
trap: CUT AT\s*[=:]?\s*0(\.0+)?\b
quote: \bat_mm\b
why: The section argument is { axis, at_mm, keep }, and `offset` is what every other
  CAD tool calls at_mm. A host that drops the unknown key cuts through the middle and
  says so only in the reply's resolved `section`, which a 5 KB reply buries. SOUND
  needs the model to report the plane the reply says was cut, not the one it asked for.
---
```

---

## L5. `threshold_mm: "1.0"` and `--set timeout_s=120` — numbers arriving as strings, twice

### (a) The moments

Over MCP:

```
### TOOL_USE mcp__ParCAD__measure_wall_thickness
{"script": "…", "threshold_mm": "1.0"}

### TOOL_RESULT
failed to deserialize parameters: invalid type: string "1.0", expected f64
```

and from the shell, thirty calls later:

```
$ parcad call measure_wall_thickness --set "script=@…/part.js" --set timeout_s=120
failed to deserialize parameters: invalid type: string "120", expected f64
```

The first cost a retry — the model dropped the threshold entirely and lost the
`below_threshold` count it had asked for. The second cost two retries and a
`python3 -c` pipeline that died on the non-JSON error text first. The model
reported it to the user:

> "One thing about ParCAD itself: number arguments passed with `parcad call
> --set` arrive as strings, and the tools reject them. `timeout_s=120` failed
> with `invalid type: string "120", expected f64`, and `threshold_mm` failed the
> same way earlier when I called the tool directly. The fix would be to parse
> `--set` values as JSON first, or have the tools accept numeric strings."

It is right on both counts.

### (b) Root cause

`crates/parcad-cli/src/call.rs:177-191`:

```rust
fn parse_set(pair: &str) -> Result<(String, Value)> {
    if let Some((key, raw)) = pair.split_once(":=") { /* JSON */ }
    let (key, raw) = pair.split_once('=')…;
    let value = match raw.strip_prefix('@') { Some(file) => read_to_string(file)?, None => raw.to_string() };
    Ok((key.to_string(), Value::String(value)))   // ← always a string
}
```

`:=` exists and does the right thing. It is documented in one place — the footer
`parcad tools` prints (call.rs:74) — and the model never ran `parcad tools`. A
flag whose only documentation is in a command the caller did not run is not
documented.

On the MCP side the schema says `number` and the model sent a string anyway.
Take that at face value: some clients stringify arguments, and a model that
writes `"1.0"` means 1.0. Every other parse on this surface is already liberal —
`Axis::parse` takes `"x"` or `"X"`, `export_part` lowercases its format.

### (c) The fix

**CLI — coerce against the schema the host already publishes.** `call()` has a
live `Client` before it sends anything (call.rs:126); one local `tools/list` is
sub-millisecond:

```rust
let listed = client.request("tools/list", json!({}))?;
let schema = schema_for(&listed, &tool);
for (key, value) in arguments.iter_mut() {
    // A bare `key=value` is text unless the tool's schema says that field is a
    // number, integer, boolean or array; `key:=json` is never touched.
    if let (Value::String(text), Some("number" | "integer" | "boolean" | "array" | "object")) =
        (&*value, schema_type(&schema, key))
    {
        if let Ok(parsed) = serde_json::from_str::<Value>(text) { *value = parsed; }
    }
}
```

Schema-directed, so `--set name=123` on a string field stays `"123"`, which a
blanket "parse anything that looks like JSON" would break. When the tool is not
in the listing, leave the value alone and say so. Also: when the host returns a
deserialize error for a value the CLI sent as a string, append one line —
``--set timeout_s=120 sent text; --set timeout_s:=120 sends the number`` — so the
escape hatch is documented where it is needed.

**MCP — accept a numeric string for a numeric field.** One helper used by every
`Option<f64>` / `Option<usize>` field in `mcp.rs`:

```rust
/// A number, or a string holding one: some clients stringify every argument,
/// and a caller that wrote "1.2" for a millimetre field meant 1.2.
fn lenient_f64<'de, D>(d: D) -> Result<Option<f64>, D::Error> { … }
```

with `#[serde(default, deserialize_with = "lenient_f64")]`. The schema still
advertises `number`, so `tools/list` is unchanged; the tool just stops refusing a
value it can read. A string that is not a number keeps a clear refusal:
`threshold_mm was "thin", which is not a number of millimetres`.

### (d) Size

CLI half a day, MCP two hours. After L4 tier 1, the best ratio on the list.

### (e) How to measure

CLI: a test in `call.rs` — `--set image_size=768` reaches the host as the number
768 and `--set name=123` as the string. MCP: a unit test that
`{"threshold_mm": "1.0"}` deserializes to `Some(1.0)`. No field case; this one is
invisible to the model when it works.

---

## L6. `.material("pla-blue")` — the error names the wrong thing, and there are no named materials

### (a) The moment

```
### TOOL_RESULT
Error: the script threw at line 59:
material colour undefined is not a hex colour; write it as "#c83c32" or "#c33"
  59 | const holder = union(spine, …).material("pla-blue");
```

2,645 bytes resent to change one string. The message says **`colour undefined`**
— it reports `material.color` on a string that has no `.color`, not the value the
caller passed. A model reading "undefined" has to work backwards to "oh, it
wanted an object".

### (b) Root cause

`app/src/dsl.ts:512-518`:

```ts
function fullHex(name: string, color: string | undefined): string {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color ?? "")?.[1];
  if (!hex) throw new Error(`material ${name} ${JSON.stringify(color)} is not a hex colour; …`);
```

called from `material()` (dsl.ts:580) as `fullHex("colour", material.color)`.
Nothing checks the *shape* of the argument first — unlike `assertEdgeSelector`,
which has `refuseSelector` for exactly this ("takes a selector, not a shape").

### (c) The fix — accept the string, and mean it

A named palette is right for a printing tool. The model reached for a filament
name because that is the vocabulary of the job: you do not buy `#2f6fb5`, you buy
blue PLA. It also fits docs/styles/field-instrument.md, which says "a part is one
colour when printed".

```ts
  /**
   * How this body looks in the window: a filament name, a hex colour, or the
   * full appearance.
   *
   * - Visual only: nothing measured reads it, and an agent's render shows it
   *   only with `materials: true`.
   * - One material per solid: a body wears the outermost one in it, a union the
   *   first operand's, a cutter's never. Two colours are two bodies.
   * - A name sets colour *and* finish — `"petg-clear"` is translucent,
   *   `"aluminium"` is metal. `MATERIALS` lists every one.
   *
   * @example return { body: box(40, 40, 10).material("pla-black"), cap: box(10, 10, 2).at(0, 0, 6).material("pla-orange") }
   */
  material(material: Material | string): Shape
```

with the table beside `METRIC_FASTENERS` and `DEVICES`, the established home for
published real-world numbers:

```ts
/**
 * Filament and metal appearances by name, for `.material()`. Colours are the
 * makers' own swatches; the finish is what the material looks like, not a
 * measurement.
 */
export const MATERIALS: Record<string, Material> = {
  "pla-white":  { color: "#eceff1", roughness: 0.65 },
  "pla-black":  { color: "#1c1c1e", roughness: 0.6 },
  "pla-grey":   { color: "#8a9099", roughness: 0.65 },
  "pla-orange": { color: "#e8702a", roughness: 0.55 },
  "pla-red":    { color: "#c8322d", roughness: 0.55 },
  "pla-blue":   { color: "#2f6fb5", roughness: 0.55 },
  "pla-green":  { color: "#3f9d52", roughness: 0.55 },
  "pla-yellow": { color: "#f0c419", roughness: 0.55 },
  "petg-clear": { color: "#dfe9ef", roughness: 0.15, opacity: 0.45, clearcoat: 1 },
  "petg-black": { color: "#202225", roughness: 0.35, clearcoat: 0.4 },
  "abs-grey":   { color: "#9aa0a6", roughness: 0.7 },
  "tpu-black":  { color: "#232323", roughness: 0.85 },
  "aluminium":  { color: "#c9ccd1", metalness: 1, roughness: 0.35 },
  "steel":      { color: "#8d9298", metalness: 1, roughness: 0.45 },
  "brass":      { color: "#c9a227", metalness: 1, roughness: 0.4 },
};
```

Under the new rule `.material("pla-blue")` simply works. An unknown name names
the near miss, the courtesy `SYNONYMS` already gives query keys:

```
material "pla-navy" is not a name or a hex colour. The nearest names are
"pla-blue" and "petg-black"; MATERIALS in read_docs `dsl` lists all 15. A colour
of your own is "#2f6fb5" or "#2b5".
```

and for the shape mistake, before `fullHex` runs:

```
material() takes a name like "pla-orange", a hex colour like "#e8702a", or
{ color, roughness, metalness }. Got an array.
```

`MATERIALS` becomes a reserved word; it is upper-case like `DEVICES` and
`METRIC_FASTENERS`, which no part would use as a local.

### (d) Size

Half a day, and it deletes the "pick a hex code" step from every styled part.

### (e) How to measure

`docs.rs`'s `every_example_runs` covers the new `@example`. Add `"pla-orange"` to
`the_reference_answers_what_a_session_had_to_guess` (docs.rs:845) — that test
exists to pin facts a session had to reverse-engineer, and this session is the
before.

---

## L7. Four consecutive fillet failures, ~10 KB each

### (a) The moments

Four builds in a row, 39,888 bytes of script, all refused:

```
Error: node 21 (line 156, untagged) fillets 10 edge(s) by 1.5 mm, and OpenCASCADE
could not build it … Largest radius measured to build on these edges: 0.52 mm …
  10.92 mm line at (-55.90, -30.90, 7.36), convex 90°
  … and 4 more
```

```
Error: node 49 (line 213, untagged) fillets 64 edge(s) by 0.5 mm … 0.25 mm …
  0.50 mm line at (-24.75, -29.90, 12.57), concave 90°
  0.50 mm line at (50.00, -29.90, 12.57), concave 90°
  … and 58 more
```

```
Error: node 49 (line 214, untagged) fillets 33 edge(s) by 0.5 mm … 0.25 mm …
  9.92 mm line at (-50.00, -32.50, 7.36), convex 95°
  9.92 mm line at (-24.75, -30.90, 7.36), convex 90°
  … and 27 more
```

```
Error: node 49 (line 215, untagged) fillets 32 edge(s) by 0.4 mm … 0.12 mm …
  9.92 mm line at (-50.00, -32.50, 7.36), convex 90°
  … and 26 more
```

The model diagnosed correctly twice and gave up on the fourth:

> "The corner rounding failed on edges only 0.5 mm long. The coin-slot cutters
> stuck 0.5 mm up into the plate and left tiny steps there."

> "The rounding still fails because the 50c slot sits only 0.5 mm from the
> angled corner."

> "The vertical slot edges still fail since the kernel can't round a corner where
> they meet the plate underside. I'll instead build the round directly into the
> cutter by flaring each slot's outer edge with a 0.8 mm quarter-circle."

### (b) Root cause — what the message does and does not know

The message is good and every part of it should stay: the bisected radius is
*measured, not estimated* (`probe_below`, backend.rs:1917; the sentence at
`repair_sentence`, backend.rs:1960), the untried interval is named as untried,
the edges are listed. It is the best refusal on this surface.

What breaks down is the **ranking**. `selection_listing` (backend.rs:883) calls
`list_edges(&described, 6)` (backend.rs:853), which sorts **shortest first** and
shows six. On failure 2 that was exactly right — six 0.50 mm edges, cause
visible, one edit. On failures 3 and 4 every listed edge was 9.92 mm and
identical: the blocker was not a short edge but a 0.5 mm *strip of face* between
a slot and a corner, and the listing had nothing to say about it. The model was
shown 6 of 32 indistinguishable edges and had to guess.

Second gap: the fillet knows `count = selected.edges.len()` at backend.rs:4894,
but the **successful** builds before these never reported it. `Treatment` in
`crates/parcad-evaluation/src/lib.rs:680-693` carries `node`, `op`, `amount_mm`,
`continuity` and no edge count, with a comment saying "what the treatment did is
`inspect_edge_target`'s answer" — i.e. another round trip. So the model could not
see `"|Z"` drifting from twelve edges to 64 as the part grew, which is what set
these failures up.

### (c) The fix — three changes, cheapest first

**1. Report the count on success (half an hour).** Add to `Treatment`:

```rust
    /// How many edges the selector actually resolved to on the shape this
    /// treatment ran against — measured, not the `.expect()` the script wrote.
    pub edges: usize,
```

"Report measured values, not requested ones", applied to the operation most
likely to drift — and the missing precondition for `.expect({ count })`, which
**no script in this session used**. A model cannot write an expectation for a
number it has never been told.

**2. Rank the listing by room, not length (half a day).** For each selected edge,
measure the smallest distance from it to another edge of its own two adjacent
faces that does not share a vertex with it — the width of the strip the rolling
ball has to live in. A bounded `BRepExtrema_DistShapeShape` per edge over a
handful of neighbours, on the failure path only. Sort by that, print it, and lead
with the count that is genuinely blocking:

```
Error: node 49 (line 214, untagged) fillets 33 edge(s) by 0.5 mm, and OpenCASCADE
could not build it (the builder reported the command not done and raised nothing).
Largest radius measured to build on these edges: 0.25 mm — rebuilt and checked, not
an estimate; reduce the fillet to that, or select fewer edges. Between 0.25 and
0.27 mm is untried.
2 of the 33 edges have less room than a 0.5 mm ball needs. The edges, least room
first:
  9.92 mm line at (-50.00, -32.50, 7.36), convex 95°, 0.50 mm to the next edge on its faces
  9.92 mm line at (-45.10, -32.50, 7.36), convex 90°, 0.62 mm to the next edge on its faces
  9.92 mm line at (-24.75, -30.90, 7.36), convex 90°, 3.10 mm
  … and 30 more, all with over 3 mm
```

That last line is the session's fourth failure solved in one read.

**3. Name the blocking edges by measurement (a day).** `probe_below` already
spends up to five rebuilds on the failure path. Spend a few more bisecting the
*edge set* at the requested radius instead of the radius over the whole set:
halve, build each half, recurse into whichever fails. Six builds isolate one
blocker among 64; a dozen isolate three. Then the message says what nothing else
here can:

```
At 0.5 mm, 30 of these 33 edges build together. The 3 that refuse, and only they,
are listed first above — exclude them with { …, longerThan: 1 } or treat them
separately.
```

Measured, not inferred, which is the standard the rest of this file holds itself
to.

**4. A pre-check without a new tool.** `inspect_treatment_target` already
resolves a treatment's edges without applying it. Give its request an optional
`radius` and its reply a `blocks` list — every edge whose room is under the
diameter — so "will this fillet build" is answerable before a 10 KB build. An
argument and a field on an existing tool, and the fillet refusal should name it:
*"inspect_treatment_target on node 49 with radius 0.5 lists them without
rebuilding."*

### (d) Size

Together two days; change 1 alone is half an hour and pays immediately.

### (e) How to measure

`eval/cases/` gains a part with one 0.5 mm face strip and the refusal's `blocks`
count recorded. Then a field case, sibling of `how-big-can-the-fillet-be`:

```
---
tool: evaluate_part.refusal
reach: evaluate_part
verdict: BLOCKING EDGES\s*[=:]?\s*2\b
trap: BLOCKING EDGES\s*[=:]?\s*(32|33|0)\b
quote: \b0\.50 mm to the next edge\b
why: A 0.5 mm fillet on 33 uniform 9.92 mm edges, two of which sit 0.5 mm from a
  corner cut. Sorted shortest-first every listed edge is identical and the refusal
  names nothing; sorted by room the two blockers are the first two lines. The trap is
  answering with the selection size, which is what a model does when it reads the
  count and not the listing.
---
```

---

## L8. The 45° corner cut: `box().rotate()` landed in the wrong place

### (a) The moment

After the first fillet failure the model suspected its corner cutter and did the
right thing — isolated it:

```js
function outline(w, d, h, cut) {
  const big = 4 * cut + 10;
  const k = w / 2 + d / 2 - cut;
  const off = k / 2 + big / (2 * Math.SQRT2);
  const corner = box(big, big, h + 1).rotate("z", 45);
  return box(w, d, h).cut(corner.at(off, off, 0), corner.at(-off, -off, 0));
}
```

> "The cut isn't landing where my maths put it, so I'm dropping that approach.
> Drawing the outline directly as a polygon will be exact."

and replaced it with a hand-written hexagon:

```js
return extrude([[-a + cut, -b], [a, -b], [a, b - cut], [a - cut, b], [-a, b], [-a, -b + cut]], h)
  .fillet(cornerR, "|Z");
```

Two extra evaluations (363 and 361 bytes — cheap, because the model was
disciplined enough to test the outline alone) on top of the 9,961-byte build that
failed because of it.

### (b) Root cause

Three lines of trigonometry to place a rotated square so one of its edges lands
on a chosen chamfer line. Nothing in the DSL says "cut this corner at 45° by
7 mm". The section vocabulary has exactly one corner treatment —
`{ at: [x, y], round: r }` (`app/src/dsl.ts:1045-1053`, `SectionEntry::Corner` at
`crates/parcad-core/src/section.rs:47`) — and no chamfer. `Shape.chamfer` exists
and would do it on a vertical edge, but it runs on a built solid, and the model
needed the shape *before* offsetting the outline inward for the skirt
(`outline(plateW - 2*s, …)`), where a treated solid cannot help.

Same shape as DSL_GAPS §2 ("selecting the outermost edges needs one selector per
corner") but on the section side: the outline vocabulary is one treatment short
of the two every plate drawing uses.

### (c) The fix — `{ at, cut }`, the chamfer twin of `round`

```ts
  /**
   * A corner cut straight across, `cut` mm back along each of the two straight
   * edges that meet there: a 45° corner on a square one, the drawing's chamfer.
   * `at` is the sharp corner where the edges would meet, not where the cut
   * starts.
   *
   * - Only between two straight edges, like `round`.
   * - Two corners on one edge must leave some of it: the refusal names the cuts
   *   that fit.
   *
   * @example
   *     // 40 × 20 plate, two 5 mm corners cut and one rounded
   *     return extrude([{ at: [-20, -10], cut: 5 }, [20, -10], { at: [20, 10], round: 3 }, { at: [-20, 10], cut: 5 }], 3);
   */
  | { at: [number, number]; cut: number }
```

Everything it needs exists: `section.rs:1393-1470` already trims the two
neighbouring edges for a round and refuses when two treatments do not fit, naming
the sizes that would; a chamfer is that arithmetic with a straight segment
instead of a tangent arc, and it cannot fail the way a radius can. Touches
`section.rs` (variant, `VOCABULARY` at section.rs:204, validation, lowering),
`dsl.ts` (type + `checkSection`), a `GRAPH_FEATURES`/`FEATURES` id, and one
`eval/cases/` entry with the area in closed form (`w·d − cut²`).

Two smaller things in the same area cover the rest of the friction:

- **`rect(w, d, corners?)`** returning a `SectionEntry[]`. The model wrote the
  same six-point outline by hand in thirteen scripts and got the arithmetic wrong
  once. `rect(115, 65, { cut: [7, 0, 7, 0] })` — per-corner, anticlockwise from
  −X−Y, a number or `{ round }`/`{ cut }` — makes "a plate with two angled
  corners" one line and composes with `inset()` for the skirt, which is literally
  what the model needed next.
- **The `chamfer` cross-reference.** `Shape.chamfer`'s doc comment is one line
  ("Bevel the edges matched by an authored selector") and never says that
  bevelling upright edges is how a plan-view corner cut is made on a solid. A
  bullet — *"an upright edge chamfered by d is the plan-view corner cut a drawing
  calls a 45°×d: `.chamfer(7, { parallel: "z", dihedral: "convex" })`"* — costs
  nothing and is where a model looks.

### (d) Size

`{ at, cut }` a day; `rect` half a day; the doc bullets an hour.

### (e) How to measure

`eval/cases/section-corner-cut` against the closed-form area, and
`every_example_runs` for the new examples. Field case: "a 115 × 65 plate with two
opposite corners cut at 45° by 7 mm, 1.5 mm rounds everywhere else",
`verdict: AREA = …`, the trap being the area a mis-placed rotated-box cutter
produces.

---

## L9. `open_project` hands back the script the caller just sent

### (a) The moment

Eleven `open_project` calls, each replying with `session::Live`, each carrying
the whole script — up to 10,636 bytes the model had sent thirty seconds earlier
in `save_project`. In the condensed transcript it shows as `"script":"<...>"`; in
the raw one `open_project` replies average 2,882 characters even truncated at
5,000, the largest reply class after `evaluate_part`.

### (b) Root cause

`crates/parcad-host/src/session.rs:39-53`, `Session.script`, flattened into `Live`
(session.rs:249) and returned whole by `open_project`, `set_script`,
`get_session` and `restore_snapshot`. For `get_session` that is the point — "the
script as it currently stands in the editor, including anything the user has
typed". For `open_project` it is an echo, and for `set_script` a verbatim echo of
the argument.

### (c) The fix

Keep `script` on `get_session` and `restore_snapshot`. On `open_project` and
`set_script`, replace it with what a caller needs — that the right text is
loaded — and let it be asked for:

```rust
    /// The script, on the tools whose job is to report what is on screen. On
    /// open_project and set_script it is what the caller just sent, so it is
    /// left out unless `script: true` asks for it; `script_chars` and
    /// `script_sha256` (first 12) identify it without carrying it.
```

`revision` and `viewers[].volume_mm3` already prove the window took it. About
30 KB of context back in this session alone.

### (d) Size

Two hours, plus one line in each of two tool descriptions.

### (e) How to measure

`mcp.rs` unit test: an `open_project` reply with no `script` key and a
`script_sha256` matching the saved file. `change-the-open-part` is the field
regression that the workflow still works without the echo.

---

## L10. `"tags": ["nub","nub","nub","nub","nub","nub","nub","nub","holder"]`

### (a) The moment

Every reply for the slab design carried that list, while `tag_extents` beside it
carried one `nub` entry with the union of all eight.

### (b) Root cause

`crates/parcad-evaluation/src/lib.rs:431`, `pub tags: Vec<String>` — one entry per
tagged *node*, not per name. Selectors match by name, so the repetition carries
nothing a selector can use; `tag_extents` is already the per-name view.

### (c) The fix

Deduplicate, keeping order, and carry the count where it is more than one — the
one fact the duplication was accidentally conveying. Either `"tags": ["nub ×8",
"holder"]`, or better for a program: leave `tags` a unique list and put
`nodes: 8` on the `tag_extents` entry, where that tag's other measured facts
already live. The doc comment becomes "Names available to selectors, each once".

### (d) Size

An hour.

### (e) How to measure

An existing `eval/cases/` snapshot with a repeated tag changes by exactly this;
`docs.rs`'s schema test covers the shape.

---

## L11. `read_docs` takes one section per call; the model made thirteen

### (a) The moment

`read_docs` with no arguments, then twelve more: `cone`, `Shape.fillet`,
`EdgeSelector`, `polar`, `Shape.rotate`, `style-field-instrument`, `tapDrill`,
`clearance`, `countersink`, `Shape.material`, `holeFor`, `extrude`. Three
(`tapDrill`, `clearance`, `countersink`) were issued in one turn, so the model
already wanted them batched. Each reply repeats the same nine-name `sections`
array — about 200 characters of pure overhead per call.

### (b) Root cause

`crates/parcad-host/src/mcp.rs:394`, `pub section: Option<String>` — one name;
`service::read_docs(topic, section, detail)` resolves one.

### (c) The fix

Accept a list and reply with the concatenation:

```rust
    /// One part of a topic, or several: a section name, one entry's name
    /// (`holeFor`, `Shape.mirror`, `SectionEntry`), or a list of them —
    /// `["holeFor", "countersink", "clearance"]` answers in one call. The
    /// reply's `section` names what came back, joined by `, `.
    #[serde(default)]
    pub section: Option<OneOrMany<String>>,
```

Bounded by the existing 12,000-character section budget (`docs.rs:112`) — entries
average under 1 KB, so a list of eight fits, and one that would not is truncated
naming the entries dropped. This is not the `query` parameter
WRITING_FOR_MODELS lists under "Not done"; it is the same call the model is
already making, eight times at once, and the two compose.

### (d) Size

Half a day including the budget test.

### (e) How to measure

`every_reply_fits_in_what_a_client_shows` (docs.rs:945) extended to the worst
plausible list. Field: `say-the-symmetry-once` re-run, checking the call count
falls without the SOUND rate moving.

---

## L12. A browser tab last seen 6,942 seconds ago is still a viewer

### (a) The moment

```json
"viewers":[{"kind":"desktop","revision":2,"seconds_ago":0,"volume_mm3":49881.688},
           {"kind":"browser","revision":1,"seconds_ago":6942,"volume_mm3":55074.791}]
```

Two hours stale, a revision behind, and a volume from a part that no longer
exists. Later in the session the model read the *browser* viewer's report as
confirmation the window had built its part — "It's saved as `euro-coin-holder`
and open in the ParCAD window, where it built" — at a moment when the desktop
window was 800 seconds behind.

### (b) Root cause

`crates/parcad-host/src/session.rs:212-228`: `viewers()` maps every entry ever
recorded; only `forget_viewer` (session.rs:203) removes one, and it needs the
tab's SSE stream to end cleanly. A tab that slept, or a laptop that closed, stays
on the list forever.

### (c) The fix

Drop a viewer that has not reported in a while, and mark the ones approaching it:

```rust
/// A window that has not reported for this long is gone as far as any caller is
/// concerned: a slept tab and a closed one look identical from here.
const FORGOTTEN: Duration = Duration::from_secs(300);
```

plus `stale: true` on anything over 30 seconds, and one sentence in
`open_project`'s and `get_session`'s descriptions: *"a viewer over 30 seconds old
is marked `stale` and is not evidence the user is looking at anything."*

### (d) Size

Two hours.

### (e) How to measure

Unit test with an injected clock; `did-the-window-draw-it` is the field
regression.

---

## L13. `.expect({ count })` in 0 of 45 scripts, and the reason is mechanical

The server instructions say *"Add `.expect({ count: n })` so a selector that
drifts fails aloud."* The model wrote 32 scripts containing fillets and never
once used it — while filleting 64, 33 and 32 edges by selectors like `"|Z"` that
had been written when the part had twelve.

The reason is not laziness. `expect({ count: n })` requires knowing `n`, and
nothing tells you `n`: `Treatment` in the reply has no edge count (L7), so the
only source is a separate `inspect_treatment_target` call per treatment. The
instruction asks for something the surface makes expensive.

Three fixes:

1. `Treatment.edges` (L7 change 1) makes `n` free — it can be pasted from a reply
   already in hand.
2. Allow a range, so an expectation can be written *before* the count is known:
   `expect({ atLeast: 1 })` is the honest form of "this must select something",
   and `expect({ atMost: 8 })` catches drift without pinning a number the author
   cannot yet know. `EdgeExpectation` (dsl.ts:113) has one field; two more are an
   hour plus the resolver's message.
3. Reword the instruction to say where `n` comes from: *"every treatment in an
   evaluate_part reply carries the number of edges it resolved to; paste it into
   `.expect({ count })` so the next edit fails aloud instead of filleting
   something else."*

Measured with a field case shaped like `say-the-symmetry-once` — the only
existing *authoring* case — whose SOUND requires `.expect(` in a script the model
actually sent.

---

## L14. Errors do not name the cheap tool that would have answered

`check_selector` (no geometry), `inspect_treatment_target` (no rebuild) and
`list_entities` were never called, and each matches a failure that did happen:
the selector grammar error (L2), the four fillet failures (L7), and "which edges
does `|Z` reach now". Descriptions are read once, at the start; an error is read
at the moment of need. Every refusal a cheaper tool could have prevented should
end with that tool's name and arguments — the fillet refusal saying
*"`inspect_treatment_target` node 49, radius 0.5 lists these without
rebuilding"*, the selector refusal saying *"`check_selector` parses one without
building anything"*. One line each, and the cheapest routing there is.

---

# Part 2 — the two structural changes

## B1. Stop sending the script: evaluate what is saved, and edit it in place

### The evidence

33 `evaluate_part` calls carrying 185,377 bytes of script. The four fillet
retries carried 9,766 / 9,840 / 9,891 / 10,391 bytes to change **2, 4 and 19
lines**. Script 03 → 04 resent 3,324 bytes to turn `rotate(0, 0, 45)` into
`rotate("z", 45)`. Script 43 → 44 resent 10,636 to change 12 lines.

Measured over all 45 scripts: as whole files, 240,779 bytes; as patches against
the cheapest of the previous six, **49,375** — 79 % less, about 48 k tokens in
one session.

And the model felt it. With one late edit to make, it went **around** the tool
surface entirely:

```
### TOOL_USE Bash
cd "/Users/leon/Library/Application Support/parcad/euro-coin-holder.parcad" &&
python3 - <<'EOF'
p = "part.js"; s = open(p).read()
old = """const panelLine = union(…"""
new = """// Round joints where the line kinks…
```

then re-measured through `parcad call measure_wall_thickness --set
script=@…/part.js`, then `open_project` to force the window to reload. Three
tools and a shell to do what one call should do. The old/new string replacement
it wrote by hand is precisely the tool that is missing.

`set_script` was never used at all: it takes the full script too, so it saves
nothing over `evaluate_part`, and its description says to call `get_session`
first — a third call, and another full script in the reply (L9).

### The design

**1. Every script-taking tool accepts a project instead.** Add to
`EvaluateRequest`, `ThicknessRequest`, `ProbeRequest`, `ExportRequest`,
`InspectRequest`:

```rust
    /// The part to build, as list_projects spells it — 'Mounts/bracket'. Give
    /// this or `script`, not both: `project` builds the saved part.js as it
    /// stands, which is what the user's window is showing, and costs no script
    /// to send. `project: "@session"` is the script open on screen, edits and
    /// all.
    #[serde(default)]
    pub project: Option<String>,
```

`projects::read` exists, and the build cache is keyed on script text
(`script::CACHE`), so a `project` evaluate hits the same cache a `save_project`
just filled — `reused_build: true`, no second kernel run. This alone removes
every save-then-re-measure pair in the session.

**2. One editing tool: old/new, against the saved part or the open one.**

```rust
#[tool(
    name = "edit_part",
    description = "Change part of a script without sending the whole thing: each edit \
      replaces `old` with `new`, once, and the part is rebuilt and measured exactly as \
      evaluate_part does — same reply, same views. `project` names the part to edit, or \
      \"@session\" for the script on the user's screen. An `old` that appears twice, or \
      not at all, is refused naming how many times it was found, and nothing is written: \
      give more surrounding lines. The previous text is kept as a snapshot \
      (list_snapshots, restore_snapshot), so an edit is undoable, and an edit to the \
      open part lands in the window's own undo history. Use it for every change after \
      the first: a fillet radius, a dimension, one function — sending 10 KB of unchanged \
      script to change two lines is the most common way a session runs out of room."
)]
struct EditRequest {
    project: String,
    /// Each { old, new }, applied in order to the text as the previous edit left it.
    edits: Vec<Edit>,
    /// Refuse if the part's current text does not hash to this — what a previous
    /// reply reported. Leave it out to edit whatever is there now.
    expect_sha256: Option<String>,
    views: Option<Vec<String>>,
    // … the rest of evaluate_part's arguments, verbatim
}
```

Semantics that matter, each learned from a moment in this transcript:

- **Refuse on ambiguity, do not guess.** `old` matching twice is refused with the
  count and the first two line numbers — "refuse rather than approximate" applied
  to text.
- **Evaluate before writing.** The edited text is built first; a refusal leaves
  `part.js` untouched, so a failed edit cannot leave the user a broken file — the
  rule `save_project`'s own description states.
- **Snapshot before writing**, through `projects::snapshot`, which
  `save_project` already calls, so `list_snapshots` / `restore_snapshot` cover
  undo with no new machinery.
- **`@session` routes through `session::set_script`**, so the edit lands in the
  window's undo history as an ordinary change.
- **The reply is `evaluate_part`'s**, plus `edits_applied` and the new
  `script_sha256`. One call replaces evaluate + save + open.

**What it would have cost this session**: the four fillet retries become
773 + 144 + 116 + 788 bytes instead of 39,888; outbound script traffic falls from
241 KB to about 50 KB.

**Size**: three to four days — the `project` argument is a day across five tools,
`edit_part` two more with the ambiguity refusals, the snapshot path and the
session route.

**How to measure**: an `eval/field/` case handing the model a saved 200-line part
and asking for one dimension changed and the new volume reported, with
`reach: edit_part` — a trial that re-sends the whole script is LUCKY at best. And
a number worth recording in docs/PERCEPTION.md: total script bytes sent per case,
before and after.

---

## B2. A part that carries its own checks, so a check cannot be "not re-run"

### The evidence

Twice, in the two most important turns of the session:

> "**Not yet re-run on this version:** the coin-fit, tilt and wall-thickness
> checks. The cups and gaps are unchanged, so I expect them to pass, but I
> haven't measured it yet."

> "**Not yet re-run on this version:** the wall-thickness check and the per-coin
> tilt check. The cup geometry hasn't changed, but I haven't measured it here."

The checks were real, careful and expensive to build: a `stacks` body of 36 coin
cylinders, a `tipped` body per denomination rotated to each cup's worst-case
tilt, a `leaving` body slid 8 mm out. They lived in **throwaway scripts** — files
38, 42 and 45 in the record — each a ~7 KB variant of the model with the
decoration stripped and the test bodies appended. Every one had to be rebuilt by
hand from the current design, and re-deriving it is why it kept not happening.

What it cost: the model found a real defect this way ("the catch only extends
0.6 mm into the 0.6 mm-oversized slot, so a coin pushed to the far wall could
slide past it untouched") and then, three designs later, shipped a version whose
tilt check had never run against it. The mechanism works. Nothing carries it
forward.

### The design: `checks` in the returned object

The constraint that shapes this is DSL_GAPS §7: every DSL export becomes a
reserved word, so `assert`, `check` and `clear` are all bad global names (and
`clearance` is taken). So add **no global at all**. A script already returns an
object of named bodies; let one key of it be data rather than a shape:

```js
return {
  top, cups, base,
  stacks, tipped_2e, tipped_1c,          // bodies the checks talk about
  checks: [
    { clear:      ["top", "stacks"],   atLeast: 0.2, why: "coins must not bind on the plate" },
    { interferes: ["cups", "tipped_1c"],              why: "a tipped 1c must still hit the arms" },
    { wall:  { min: 1.0 }, ignore: ["feather"] },
    { size:  { max: [115, 65, 15] } },
    { standsOn: { atLeast: 0.3 } },
  ],
};
```

- **No new reserved word.** The only thing lost is a *body* named `checks`, and
  the refusal for that is one sentence.
- **No new graph op.** `checks` is plain JSON, stamped onto the graph as a
  top-level list beside `requires` — `envelope.rs:120` already gates the
  top-level key set, so it needs an entry there and a `FEATURES` id.
- **Nothing new to compute.** `clear` / `interferes` read `between_bodies`, which
  `evaluate_part` already measures for every pair (transcript:
  `{"a":"holder","b":"stacks","clearance_mm":0.01,"verdict":"clear"}`). `size`
  and `standsOn` read the snapshot. Only `wall` costs anything, and it is
  `service::wall_thickness` against the same cached build.
- **The reply cannot omit it**, which is the whole point:

```json
"checks": {
  "verdict": "failed",
  "passed": 4,
  "failed": [{
     "check": "clear top↔stacks atLeast 0.2",
     "measured_mm": 0.13,
     "at": [[25.95, 21.675, 12.32], [25.95, 21.675, 12.19]],
     "why": "coins must not bind on the plate"
  }]
}
```

with `checks: { verdict: "passed", passed: 5 }` on success — four words when
there is nothing to say, rule 2 of WRITING_FOR_MODELS.

### What a failure does

Not a refusal at `evaluate_part`: a model exploring a change needs the picture of
the part that failed. So:

- `evaluate_part` returns normally, with `checks.verdict` **first in the reply**
  and the failing check named.
- `export_part` and `save_project` **refuse** a part whose checks fail, naming
  the check and the measurement, unless given `allow_failing: "<reason>"` — the
  same door, the same shape, as the `allow_thin: "reason"` docs/NEXT.md item 1
  specifies for `print_check`.

That is the deliberate seam with NEXT.md item 1: **`print_check` is the check
nobody wrote; `checks` is the check the author wrote.** They share the verdict
slot in the reply and the refusal at the export door, and neither can be skipped
by forgetting. One implements the other's machinery; doing them in one session is
cheaper than doing either alone.

### The vocabulary, kept small on purpose

| entry | reads | means |
|---|---|---|
| `{ clear: [a, b], atLeast }` | `between_bodies` | the two never touch, by at least this much |
| `{ interferes: [a, b], atLeast }` | `between_bodies` | they *must* overlap — a catch that has to catch |
| `{ touching: [a, b] }` | `between_bodies` | flush: neither gap nor overlap |
| `{ wall: { min }, ignore, on }` | `wall_thickness` | nothing thinner than this; `ignore: ["feather"]` for intended knife edges, `on: ["cups"]` to scope it to a tag |
| `{ size: { max: [x, y, z] } }` | the snapshot | fits this envelope as drawn |
| `{ standsOn: { atLeast } }` | `stands_on` | this fraction of the footprint reaches the bed |
| `{ bodies: n }` / `{ watertight: true }` | the snapshot | the part did not quietly split |

Every one is a sentence somebody said out loud in this session. `why` is free
text echoed back in the failure, which is how the reply explains itself to the
user without the model re-deriving the intent.

Author-time validation reuses `queryShapeError`'s machinery
(`app/src/selectors.ts:170`): an unknown key named, with the synonym suggested
(`clearance → atLeast`, `interfere → interferes`, `thickness → wall`).

**Size**: three to five days, most of it reply plumbing and the export door,
shared with NEXT.md item 1.

**How to measure**: `eval/cases/` gains a part whose `checks` pass and one whose
`clear` fails by a recorded 0.13 mm. Then the field case that matters:

```
---
tool: evaluate_part.checks
reach: evaluate_part
verdict: FAILING CHECK\s*[=:]?\s*clear top\b
trap: FAILING CHECK\s*[=:]?\s*(none|wall)\b
quote: \b0\.13\b
why: The part carries five checks in its returned object and one fails by 0.13 mm
  against an atLeast of 0.2. Every other number in the reply passes: it is watertight,
  one body per name, prints on every bed. A model that reports the part as fine read
  the geometry and not the verdict — the failure this feature exists to make
  impossible, seen twice in the session that motivated it.
---
```

---

# Part 3 — ranking

Round trips counted as "this session, plus how often the shape recurs". Effort is
wall-clock for one session on this codebase.

| # | change | effort | round trips saved | notes |
|---|---|---|---|---|
| 1 | **L4 tier 1** — `deny_unknown_fields` on all 18 request structs | 1 h | 1 here, plus one *silently wrong measurement* prevented | Best ratio on the list, and the only item that fixes a wrong answer rather than a retry |
| 2 | **L7 change 1** — `Treatment.edges` in the reply | 0.5 h | unlocks `.expect()` (0 of 45 scripts) | Measured value the kernel already holds at backend.rs:4894 |
| 3 | **L1** — name the shadowed builtin | 0.5 d | 1 retry + 1 wasted `read_docs`, in every session that names a local `hole`/`clearance`/`grid`/`device` | DSL_GAPS §7 specifies it; proof-by-recompile makes it exact |
| 4 | **L3** — `rotate`/`mirror`/`scale` argument checks + scope `if_newer()` | 0.5 d | 1 retry + 1 false bug report to the user | The OpenSCAD form is the likeliest wrong call on this surface |
| 5 | **L5** — CLI schema coercion + lenient numbers over MCP | 0.5 d | 2 retries + a failed shell pipeline | Model reported it as a defect; it is one |
| 6 | **L10 / L12** — dedupe `tags`, expire stale viewers | 0.5 d | 0 direct; removes two ways to read a reply wrong | Cheap correctness |
| 7 | **L2 message** — selector error points at the query form | 0.5 d | 1 retry, and routes to `check_selector` | The corpus re-record is the real work |
| 8 | **L9** — stop echoing the script from `open_project`/`set_script` | 2 h | ~30 KB of context back | Pure reply-size win |
| 9 | **L6** — named materials + a message naming what was passed | 0.5 d | 1 retry, and deletes a step from every styled part | `MATERIALS` beside `DEVICES` |
| 10 | **L11** — `read_docs` takes a list of sections | 0.5 d | ~8 calls of 13 | Composes with the queued `query` |
| 11 | **L14** — three refusals name the cheap tool that answers them | 1 h | routes to three tools never called | One line each |
| 12 | **L13** — `expect({ atLeast, atMost })` + reworded instruction | 0.5 d | prevents the L7 class rather than reporting it | Needs #2 first |
| 13 | **L8** — `{ at, cut }`, `rect()`, the `chamfer` bullet | 1.5 d | 2 retries here; every plate outline afterwards | Section machinery already does the hard half |
| 14 | **L7 changes 2–3** — rank edges by room, bisect the edge set | 2 d | 3 of the 4 fillet retries | The best refusal on the surface, ranked by the wrong key |
| 15 | **B1** — `project` argument + `edit_part` | 3–4 d | 79 % of 241 KB of script traffic | The one change that alters the shape of a session |
| 16 | **B2** — `checks` in the returned object | 3–5 d | 0 round trips; removes two "not re-run" admissions | Shares the export door with NEXT.md item 1 |

Items 1–5 are one day together and cover five of the eleven lost round trips. If
only one larger thing is done, **B1** — it multiplies the value of every error
message above by making the retry cheap.

---

# Appendix — what belongs in the docs

- **docs/WRITING_FOR_MODELS.md, "What parcad measured"**: *"A rule in the
  preamble does not prevent the collision it describes. The `dsl` contents page
  says every export is a reserved word and lists `clearance`; the session that
  read it declared `const clearance` eleven calls later. A hazard a caller can
  trip on belongs in the error, not only in the reference."*
- **docs/DSL_GAPS.md §7**: the reserved-word entry gains a second, dated instance
  with the cost attached (one retry, one wasted `read_docs`, 3.2 KB).
- **docs/GOTCHAS.md**: an unknown MCP argument is silently dropped — `section`
  with `offset` cuts through the middle and says so only in the reply's resolved
  `section`. Until L4 lands that is a gotcha; after it, delete the entry.
- **docs/PERCEPTION.md**: worth a column — **script bytes sent per field case**,
  before and after B1. It is the one cost this surface never measures.


---

# Appendix C — workflow / UX lens

# The pocket coin holder, read as a workflow

What ParCAD should build so a conversation between a user and a model designing
a part converges in fewer turns, wastes less work, and ends with a printed
object. Every proposal is anchored in a moment of session
`0a039327-b978-439a-ad6f-f5b8287c3109` (2026-09-18, 21:14-22:08).

Nothing here re-proposes NEXT.md's queue. Where a proposal upgrades a queued
item -- section 1's `print_check` on the export route, section 2's "a render is
colourless unless asked", item 5's parts library -- it says so and says what it
adds.

---

## What the session actually cost

Measured from the transcript and the scripts on disk, not estimated:

| | |
|---|---|
| user turns | 6 (`build it`, `but i said pocket`, `overhangs / multipart`, `teenage engineering`, `align to edges / make it snap`, `its too big`) |
| tool calls | 78 -- 33 `evaluate_part`, 13 `read_docs`, 10 Bash, 7 `open_project`, 6 `save_project`, 5 `measure_wall_thickness`, 1 each `probe_part`, `read_project`, `list_projects` |
| `export_part` calls | **0** |
| script text sent | **241 KB across 45 calls** (`scratchpad/scripts/`) |
| designs built from scratch | 5 (brick -> letterbox -> C-cups -> screwed sandwich -> snap sandwich -> compact) |
| wall clock | ~55 min |
| object printed | none |

Three numbers underneath those:

- **Nineteen of the 45 script sends changed fewer than 10 lines.** Calls 03-06
  each resent 3.3 KB to change 2-4 lines; 35 and 36 each resent ~9.9 KB to
  change 3 and 7 lines. There is no way to build a *saved project by name* --
  `EvaluateRequest` takes only `script: String`
  (`crates/parcad-host/src/mcp.rs:190`).
- **The verification rig is a second, divergent copy of the part.**
  `scripts/42-evaluate_part.js` is `scripts/44-save_project.js` with 15 lines of
  coin fixtures appended and the whole file re-minified to one-liners. The model
  maintained a pretty copy for saving and a flat copy for checking, and resent
  7.1 KB of it per check. It kept them separate because the language has no way
  to say *this body is a reference, not part of the part* -- a named body is
  measured, drawn, **and exported**.
- **"Compacter" got bigger in plan.** v2 (the C-cups the user answered `nice`
  to) is 105.5 x 41.5 x 11.9 mm, 8.3 cm3. v5, built in answer to "its too big",
  is 110 x 56.9 x 10.7 mm, 18.2 cm3 -- 4.5 mm longer and 15 mm wider than the
  version the user liked. Nothing in any reply compared the two, because v2
  existed only as `.history/euro-coin-holder/1789760158499.js`.

And the ending: the model closed turns 1, 2, 3, 4 and 5 with a variant of
*"shall I export a 3MF and open it in your slicer?"*. The user answered none of
them. Five offers, zero files.

---

## 1. The brief -- *build this first*

**(a) The moment.** Message 1 said "pocket". The first build was
97.3 x 55.8 x 20.2 mm, 49.9 cm3, and every number in its report -- watertight,
`stands_on` 74 %, `prints_on` all three beds -- was green. Message 2 was *"but i
said pocket"*. Message 6 was *"its too big"*. Between them the model built five
designs against a requirement that lived only in the first four words of the
conversation. ParCAD holds a part's *units* so a file cannot be misread
(`Doc::units`) and holds nothing at all about what the part is for.

**(b) The surface.** A `brief()` in the DSL, recorded on the `Doc` the way units
are, and reported against on every build. Not a tool call the model can forget --
a declaration in the part, so it survives a redesign, a `.history` snapshot and
a hand edit.

```js
brief({
  envelope: [95, 70, 16],          // must fit inside this, any orientation
  budget_cm3: 12,                  // plastic
  holds: ["3 x 2EUR", "4 x 1c"],   // prose; the checks below are the measurement
  gesture: "one hand, thumb only", // prose, for the model and the window
  printer: "Bambu A1 mini",        // narrows prints_on to one bed
  material: "PETG",
});
```

`envelope` is the one that had to exist first: a pocket is a size, and a size is
the thing five reports never judged. Ship a measured `POCKETS` table beside
`DEVICES` in `dsl.ts` so "pocket" is not a guess --
`envelope: pocket("jeans-front")` -- with the same honesty label `DEVICES` uses
("measured off ..., +-N mm").

`evaluate_part`'s reply gains one key, verdict first:

```json
"brief": {
  "verdict": "over: 110.0 mm along x against a 95 mm envelope; 18.2 cm3 against 12",
  "envelope": { "given": [95, 70, 16], "measured": [110.0, 56.9, 10.7],
                "fits": false, "over_mm": 15.0, "on": "x",
                "note": "measured in the best of the 6 axis orientations" },
  "budget_cm3": { "given": 12, "measured": 18.2, "over": 6.2 },
  "holds": ["3 x 2EUR", "4 x 1c"],
  "gesture": "one hand, thumb only"
}
```

With no `brief()` in the script the key is still there, once:
`"brief": { "none": "no brief() in this script; size and volume are measured
against nothing" }`. That line is the whole nudge -- the model reads it on build
one of turn one and writes the brief it just heard.

The window shows it as a fifth line in `report.tsx`, red when over, in the same
place `stands_on` turns red: `95 x 70 x 16 envelope -- over by 15.0 along x`.

**(c) Files and size.** `app/src/dsl.ts` (`brief`, `POCKETS`, a reserved-word
check), `crates/parcad-core/src/graph.rs` (`Doc::brief`), a new
`crates/parcad-core/src/brief.rs` (~120 lines: the 6-orientation envelope fit and
the volume compare, both closed-form over `bounds` and `volume_mm3`),
`measure.rs`/snapshot, `mcp.rs` (three sentences in `evaluate_part`'s
description), `app/src/ui/report.tsx`, one `eval/cases/` entry. **~450 lines,
1.5 days** including the pockets table.

**(d) Proof.** The field case in section 11 below, plus a replay: re-run turn 1's
prompt with `brief()` available and check that the reply's first line is a
verdict against 95 mm rather than a description of a 97 mm brick. The
before/after is binary -- turn 2 and turn 6 were both requirement corrections,
2 of 6 user turns.

**Ship the render's half in the same session.** Every one of the 12 renders the
user judged is an object floating on black at unknown scale. A 50 mm rule along
the bottom edge of every raster view, labelled in millimetres, costs an hour in
`render.rs` and is the same fact as the brief in the channel the user actually
looked at. "But I said pocket" is a scale judgement made from a picture with no
scale in it.

---

## 2. Checks that live in the part

**(a) The moment.** Twice -- end of turn 4 and end of turn 5 -- the model wrote
*"Not yet re-run on this version: the wall-thickness check and the per-coin tilt
check."* It had done genuinely expensive, honest verification (coin stacks,
tipped coins at each mouth's worst angle, a sliding coin, wall thickness) and
then shipped a design whose checks were one revision stale, and said so. It
happened because each check was a throwaway 7 KB script, not a property of the
part.

**(b) The surface.** A third kind of body: a **reference**. Measured against the
part's bodies on every build, drawn in a fixed reference colour, never exported,
never counted in `bodies` or `volume_mm3`.

```js
return {
  top, cups, base,
  checks: {
    "a full stack clears the lid":  clear(stacks, top, { atLeast: 0.2 }),
    "a tipped 2EUR cannot leave":   interferes(tip2e, cups),
    "a coin sliding out clears":    clear(leaving, top),
  },
};
```

`clear(a, b, {atLeast})` and `interferes(a, b, {atLeast_mm3})` are declarations,
not calls -- the kernel already computes every pair in `between_bodies`; this
names the pairs that matter and what they must say. The reply:

```json
"checks": {
  "verdict": "3 of 3 hold, on this build",
  "held": [
    {"name":"a full stack clears the lid","want":"clear >= 0.2","got":"clear 0.40 mm at [-25.7, -15.7, 9.54]"},
    {"name":"a tipped 2EUR cannot leave","want":"interferes","got":"interferes 1.23 mm3"},
    {"name":"a coin sliding out clears","want":"clear","got":"clear 1.17 mm"}
  ],
  "broken": []
}
```

A broken check names the fix the way a refusal does:
`"want clear >= 0.2, got clear 0.05 mm at [48.1, -28.8, 5.75] -- between cups and
stacks; widen coinGap or lower innerH"`.

Three things fall out at once. The "not yet re-run" sentence becomes
structurally impossible -- checks re-run on every build, for free, off the
`between_bodies` pass that already runs. The 7 KB duplicate script disappears,
and with it the pretty-copy/flat-copy drift. And the *user* sees the checks: a
line in `report.tsx`, green or red, while they watch the part rebuild.

**(c) Files.** `app/src/dsl.ts` (`clear`, `interferes`, the `checks` key on the
returned object -- three new reserved words, so check `examples/` builds),
`graph.rs` (`Op::Bodies` gains `references` and `checks`), `backend.rs` +
`measure.rs` (exclude references from `bodies`, `volume_mm3`, `stands_on`,
`prints_on`; keep them in the pair pass), `service.rs` (`export_stl`/`_3mf`/
`_step` and `body_doc` drop them), `render.rs` (reference colour),
`report.tsx`, `mcp.rs`. **~550 lines, 2 days.** The largest item here and the
one with the widest blast radius -- `Op::Bodies` is exhaustively matched, which
is the compiler finding the rest for you.

**(d) Proof.** `eval/cases/` entry with a part whose check is red on a recorded
number. Field case: a part with a failing check, asked "is this ready to
print?" -- SOUND only when the reply names the failing check and its two bodies.
On this transcript: 8 of the 33 `evaluate_part` calls were check rigs; all 8
collapse into the builds that were happening anyway.

---

## 3. Overhang, measured -- and a view of the part as it prints

**(a) The moment.** Turn 3, in full: *"nice BUT you have overhangs 3d printer
would struggle with. consider leveraging multipart nature."* The user did, by
eye, a job ParCAD has no tool for. The overhanging lips had survived 13
`evaluate_part` calls, each one reporting `prints_on: fits: true` on three beds
and `stands_on: 0.228` -- every printability number ParCAD has, all green, about
a part that cannot be printed as drawn. PERCEPTION section 6 records overhang as
the open half of printability and has since before this session.

**(b) The surface.** In every `evaluate_part` reply, per body, in its as-drawn
build direction:

```json
"overhangs": {
  "verdict": "214 mm2 unsupported, worst 0 deg -- the lips, tag `lip`",
  "unsupported_mm2": 214.3,
  "threshold_deg": 45,
  "places": [
    {"tag":"lip","angle_deg":0.0,"area_mm2":186.2,"at":[-47.2,-19.1,4.9],"spans_mm":1.5},
    {"tag":"barb","angle_deg":44.9,"area_mm2":28.1,"at":[33.5,0,2.0],"spans_mm":0.4}
  ],
  "bridges": [{"tag":"slot","spans_mm":12.4,"at":[2.5,-29.9,7.4]}],
  "note": "the angle each face makes with the bed, measured on the exact solid, built +Z. A face at or under threshold_deg needs support or a bridge. This is a measurement, not a verdict on your printer."
}
```

Report the geometry, not the verdict -- PERCEPTION section 6's own rule, and the
reason `spans_mm` is there: a 1.5 mm inward lip and a 40 mm ceiling are both
0 degrees, and only one of them is a problem.

And a **print view**: `views: ["print"]`, which draws each body resting on the
face it would print on, seen from the bed, unsupported area shaded. Turn 3's and
turn 5's designs are three and three flat parts; assembled renders are the wrong
picture for the question the user was asking.

**(c) Files.** `crates/parcad-occt/src/perceive.rs` (face normals and areas off
the exact solid -- already available where wall thickness samples), snapshot,
`render.rs` (the print view and the shading), `mcp.rs`, `report.tsx`.
**~350 lines, 1 day.**

**(d) Proof.** Field case `where-does-it-need-support` (section 11), and an
`eval/cases/` entry pinning the mm2 on a part with a known lip. Replay: turn 3's
first half is a user doing a measurement; with this it is a line in turn 1's
reply.

---

## 4. The ending: a `print` route instead of a closing question

**(a) The moment.** Five turns, five closing offers, zero exports. *"Does the
look work for you? Once you're happy with it I'll export a 3MF and open it in
your slicer"* (turn 1) ... *"Do you want the 3 mm longer version with proper
corner cuts, or should I export the three parts as 3MF files now?"* (turn 5).
The user's next message was `give your sesh id`. A question at the end of a long
reply is where a conversation goes to stop.

**(b) The surface.** Two changes, and the first is the one that matters.

**Stop asking. Save produces the files.** `save_project` on a part that builds
writes `print/<body>.3mf` inside the `.parcad` folder, laid flat, one file per
body, colours from `.material()` as 3MF `basematerials`, and returns their
paths. Derived and disposable like `preview.png` and `README.md`, rewritten on
every save, and nothing reads them back as fact -- exactly the rule
`crates/parcad-host/src/projects.rs` already states for that folder. The
reply's first line becomes a statement, not a question:

```json
"print": {
  "verdict": "3 files ready, laid flat, 0 mm2 unsupported",
  "files": [
    {"body":"top","path":".../euro-coin-holder.parcad/print/top.3mf","laid":"top face to the bed (turned 180 deg about x)","unsupported_mm2":0,"thinnest_wall_mm":1.2,"bed":"Bambu A1 mini"},
    {"body":"cups","path":".../print/cups.3mf","laid":"as drawn","unsupported_mm2":0,"thinnest_wall_mm":1.0},
    {"body":"base","path":".../print/base.3mf","laid":"as drawn","unsupported_mm2":0,"thinnest_wall_mm":2.0}
  ],
  "flagged": ["cups: the pin prongs read 0.58 mm at the barb split, under the 1.2 mm process minimum -- PETG, or raise barbOver"],
  "note": "tell the user these paths. `export_part` with open: true hands one to their slicer."
}
```

**A Print button in the window.** The titlebar already opens the part in the
user's editor (`service::open_in_editor`, the pencil in `titlebar.tsx`). The
same shape, one icon over: hand `print/` to the slicer. The user who never
answers a question in chat will press a button in the window they are already
watching.

This **upgrades NEXT.md section 1's third bullet** -- "`export_part` and
`save_project` carry a `print_check` ... its verdict the first line of the
reply". Queued is the *check* on the save route. What this adds: the *files*,
laid flat per body, coloured, on the same route, so that "ready to print" is a
set of paths rather than a clean verdict; and the button, so the ending does not
depend on the model asking or the user answering.

**(c) Files.** `service.rs` (per-body flat orientation -- the face with the
largest planar area normal to a principal axis, then rotate it to -Z; reuse
`export_3mf` and `body_doc`), `projects.rs` (the `print/` folder and its
rewrite-on-save rule), `mcp.rs`, `app/src/ui/titlebar.tsx`, `icons.tsx`.
**~450 lines, 1.5 days.**

**(d) Proof.** `eval/field/one-file-for-the-slicer.md` exists and is marked
**not yet run** -- run it first, as the before. Then a new case: a three-body
part, "give me what I print", SOUND only when the reply names three paths. And
the honest end-to-end: open one of these in Bambu Studio and look, which
NEXT.md's integrations section records has never been done.

---

## 5. Variants in one call

**(a) The moment.** Each of the five user pushes produced exactly one new design,
built from scratch, and the user could only say yes or push again. The most
expensive instance: turn 2's C-cups (8.3 cm3, one piece, `nice`) and turn 6's
compact sandwich (18.2 cm3, three pieces) are both answers to "make it smaller",
built 35 minutes apart, never compared.

**(b) The surface.**

```
compare_variants({
  variants: [ { name: "c-cups", script: "..." }, { name: "sandwich", script: "..." } ],
  views: ["iso"],          // one contact-sheet image, variants left to right
  timeout_s: 60
})
```

Reply -- rules first, one row per variant, nothing saved:

```json
{
  "note": "measured per variant on its own build; nothing was saved and nothing is on screen. save_project the one the user picks.",
  "columns": "size_mm, volume_cm3, bodies, thinnest_wall_mm, unsupported_mm2, brief",
  "variants": [
    {"name":"c-cups",  "size_mm":[105.5,41.5,11.9],"volume_cm3":8.3, "bodies":1,"thinnest_wall_mm":1.0, "unsupported_mm2":214,"brief":"over 10.5 mm along x"},
    {"name":"sandwich","size_mm":[110.0,56.9,10.7],"volume_cm3":18.2,"bodies":3,"thinnest_wall_mm":0.58,"unsupported_mm2":0,  "brief":"over 15.0 mm along x"}
  ],
  "views":[{"view":"iso","markdown":"![c-cups | sandwich](/.../renders/...-contact-iso.png)"}]
}
```

One image, one paste, and the user picks from two pictures side by side instead
of judging one object against a memory of another. The columns are exactly the
axes the user pushed on across six turns: size, plastic, part count, wall,
overhang, brief.

Cap it at four variants and refuse a fifth naming the cost -- this is n builds
plus n wall-thickness passes.

**(c) Files.** `mcp.rs` (one tool), a new `crates/parcad-host/src/compare.rs`
(~150 lines, all of it calling `service::evaluate`, `service::wall_thickness`
and `service::render` in a loop), `render.rs` (the contact sheet: shared
framing across variants, which `framing_bounds` already makes cheap).
**~300 lines, 1 day.** Depends on section 1 for the `brief` column and section 3
for `unsupported_mm2`, and is worth building after both.

**(d) Proof.** Field case: two scripts, "which of these uses less plastic and
which has the thinner wall" -- plausibly and wrongly answerable from the source,
since the thin wall is a fillet interaction and not a variable. Replay: turns 2,
4 and 6 each become *propose two, user picks*.

---

## 6. A version strip, and variants as siblings

**(a) The moment.** `save_project { name: "euro-coin-holder" }` six times. The
version the user liked best is
`~/Library/Application Support/parcad/.history/euro-coin-holder/1789760158499.js`
-- 2554 bytes, reachable only by `list_snapshots` plus an epoch-millisecond id,
and invisible in the picker, which skips dotted directories by design
(`projects.rs`, `walk`). When the user said "its too big" in turn 6, going back
to the 8.3 cm3 one-piece was not a move anyone could see.

**(b) The surface.** Three small things, in order of value:

1. **`list_snapshots` reports what each version *was*.** `save_project` already
   builds the script to write `preview.png`; write a one-line measured summary
   beside the snapshot at the same moment -- `{size, volume_mm3, bodies}` -- and
   serve it. Then the model can say *"the version you called nice was
   105.5 x 41.5 x 11.9, 8.3 cm3, one piece"* without rebuilding anything.
   ~60 lines, half a day, and it is the cheapest item in this document.
2. **The picker shows a version strip.** Under a selected part, its `.history`
   as thumbnails with size and volume, newest right, click to restore. The
   thumbnails need `save_project` to keep the preview it already rendered
   (`projects.rs`, one extra write). The user judges from pictures; this is the
   only proposal here that puts *their own past* in pictures.
3. **Tell the model to fork.** One sentence in `save_project`'s description:
   *"A redesign the user may want to compare against, or go back to, is a new
   name -- `euro-coin-holder/c-cups` -- not an overwrite. Overwriting is for an
   edit to the same design."* Folders already work; nothing is built. A
   sentence.

**(c) Files.** `projects.rs` (snapshot sidecar + preview), `mcp.rs`
(`list_snapshots` reply + the sentence), `app/src/ui/project-browser.tsx`.
**~300 lines, 1 day**, and item 1 alone is 60 lines.

**(d) Proof.** Field case for item 3 (an authoring case: `input` matching a
`name` with a `/` in it after a "try a different approach" prompt). For item 2,
the replay is qualitative -- but the question "would turn 6 have been *go back
to the C-cups*" is answerable by asking the user.

---

## 7. Edit a saved part, do not resend it

**(a) The moment.** 241 KB of script text across 45 calls, nineteen of which
changed under ten lines. Calls 32 and 33 are 363 and 361 bytes -- a two-line
probe of an outline -- sandwiched between 10 KB resends. The wall-thickness
checks resent the entire 7 KB part-plus-coins blob three times.

**(b) The surface.** Every tool that takes `script` takes, instead, either
`script` or `project` plus optional `edits`:

```
evaluate_part({ project: "euro-coin-holder", edits: [
  { old: "const topT = 1.5;", new: "const topT = 1.2;" }
], views: ["iso"] })
```

Same semantics as this harness's own `Edit`: `old` must appear exactly once or
the call is refused naming the count. The edits apply to the file on disk and
**are not written back** -- the reply carries the resulting script's hash and a
`script_after` only on request, so the model stays honest about what was built.
A `save: true` writes it (and snapshots, as save already does).

The build cache keys on the script, so this is free on the kernel side.

**(c) Files.** `mcp.rs` (a shared `Source` enum on four request structs and the
text in four descriptions), `service.rs`/`projects.rs` (resolve + apply).
**~250 lines, half a day.**

**(d) Proof.** Replay this exact transcript with `project` + `edits` available
and count the bytes: 241 KB -> an estimated 35 KB, with the five full sends being
the five genuinely new designs. Field case: an editing case scored by `input`
(does a call carry `edits` rather than a 200-line `script`) after "make the
plate 1.2 mm instead of 1.5".

**This is the answer to "which single change halves the time".** Of ~55 minutes,
the largest single block is the model *writing* ~241 KB -- roughly 60k tokens of
generated script, most of it text it had already generated once. Cutting that by
~80 % is worth on the order of 15-20 minutes, and nothing else on this list
touches that block. Section 2 is second: it deletes 8 of 33 builds and the
duplicate copy of the part they carried.

---

## 8. Styles: listed, named, and honest about hardware

**(a) The moment.** Turn 4, "make it in the style of teenage engineering
aestetic". The model guessed a `read_docs` topic name -- `style-field-instrument`
-- and got lucky; the topic is real (`docs.rs:70`), and nothing lists it. It then
read `field-instrument/control-box` for house style, and the turn-4 result is
the best-looking thing in the session. Then in turn 6 it added *"a ring over
each snap pin with the panel line crossing it like a screwdriver slot"* and, in
turn 5, *"diagonal hazard stripes at the 2EUR end"* which it measured as reading
"more as tick marks".

**Whose fault?** Split, and the split is instructive. The style doc forbids
"a decorative feature with no job" and "a second accent", so the hazard stripes
are the model's. But the doc forbids *faking letters from boxes* and says
nothing about faking *hardware*, and the fake screw heads are the hole in the
doc -- a rule a caller can get wrong that is not written down, which is exactly
rule 8 of WRITING_FOR_MODELS. And nothing in the doc answers turn 5's "align to
edges - more UX", because the doc is about proportions, edges and layout and has
no section on how a hand takes hold of the thing.

**(b) The surface.**

- **`read_docs` topic `styles`** -- one line per style, so a style is found by
  listing rather than guessing. Five lines of `docs.rs`.
- **Two new sections in every style doc.** **Gestures**: what a hand does to
  this part -- which finger, one-handed or two, thumb reach, what it rests on
  while you work it, and the numbers (a thumb pad is ~18 mm; a pinch needs
  ~6 mm of coin proud of the plate). That is what "more UX" was asking for and
  there was nowhere to look it up. **Do not fake hardware**: no drawn screw
  heads, no fake vents, no hazard stripes, no panel lines that are not a real
  parting line -- the same rule as "do not fake letters from boxes", generalised
  to the thing the model actually faked.
- **A named-material palette.** In turn 2 the model wrote
  `.material("pla-blue")` and got *"material colour undefined is not a hex
  colour"*. It expected names; the language has hex. Give each style a table --
  `material("shell")`, `material("accent")`, `material("base")` -- so a style's
  colours are data rather than three hex literals guessed per part, and so
  "white top, orange cups, black base" is checkable. (The v4 render shows a grey
  top, not white; nothing said so.)
- **`style` on `save_project`**, recorded in `parcad.json` beside `title`. It
  costs two fields and it means a part carries what it was trying to look like
  into the next session.

**Other styles worth writing**: *shop fixture* (jigs, soft jaws, workholding --
datum faces, clamping, nothing cosmetic), *print-native* (the 45 degree rule as
a design language, layer direction as a structural decision, bridges and
chamfers instead of supports -- this session needed it twice), *domestic object*
(wipeable, no crevices, radii sized to hands). **Not worth writing**: a style per
brand. "Teenage engineering" resolved to `field-instrument` correctly and
without copying anyone; a `docs/styles/teenage-engineering.md` would be a
licence and taste problem, and the existing doc's "use the style's rules, not
anyone's product" is the right answer. Add an alias table instead -- the words
users actually say ("teenage engineering", "like a synth", "lab instrument",
"braun", "muji") mapping to the style they mean.

**(c) Files.** `docs/styles/*.md` (prose, two new sections each),
`crates/parcad-host/src/docs.rs` (the `styles` topic + aliases), `app/src/dsl.ts`
(named materials), `projects.rs` (manifest field). **~200 lines + prose, 1 day.**

**(d) Proof.** Field case: "make me a small desk thing in the style of a lab
instrument" graded on `reach: read_docs` with `arg: topic` -- does a small model
find the style at all without being told its name. That is the measurement that
justifies the `styles` topic; the alias table is measured the same way with the
brand words.

---

## 9. What to look at, and what to measure instead

**(a) The moments.** Turn 2's reply says *"The pictures below have 2 coins in
each cup."* They do -- and the render is grey-on-grey, because `materials`
defaults off; the coins are indistinguishable discs. Worse, the *window* showed
no coins at all: `save_project` had written the holder-only script, so the user
watching ParCAD and the user reading the chat were looking at two different
parts while one sentence described both. Turn 5's final top view -- the picture
the session ends on -- reads as a row of battlements; the U-slots opening to both
long edges make the top plate unreadable as an object.

**(b) The surface**, in descending order of confidence:

1. **A scale rule on every raster view.** An hour. Section 1 argues it.
2. **Reference bodies in a reference colour, always** (section 2). A coin drawn
   to check a fit is not the part, and should never be the same grey as the part,
   in any flag state. This **upgrades NEXT.md section 2's first bullet** --
   queued is "the snapshot should say a part has materials, and the render note
   should name the flag", which fixes a model's blindness to its own colours.
   What this adds is that a *reference* body is never subject to that flag at
   all.
3. **An exploded view.** `views: ["exploded"]` -- named bodies pulled apart along
   the axis that separates them, by a fixed fraction of the part's extent, with
   the numbers unchanged. Turn 3's three plates and turn 5's snap sandwich were
   both shown assembled, where the mechanism the user was being asked to judge
   is invisible. Half a day in `render.rs` (the transform is per body, the
   framing is shared).
4. **A print view** (section 3).

**What should be measured rather than looked at**: overhang (section 3 -- the
user did it by eye in turn 3); scale against the brief (section 1 -- the user did
it by eye twice); whether a check still holds (section 2 -- the model said it did
not know); which body is which colour (a `materials` list in the snapshot,
NEXT.md section 2).

**What should not be built**: a rubric the model scores its own render against.
PERCEPTION section 7 already records the failure mode -- *"a section is so
convincing that it stops the model measuring"* -- and a rubric is a way of making
a picture more convincing, not more true. "Align to edges - more UX" is a taste
judgement and belongs to the user; the job is to get the picture in front of them
faster and with a scale on it, not to have the model grade itself.

---

## 10. Small things the transcript shows

**`report_issue`.** The model ended turn 1 and turn 6 with unprompted, accurate
ParCAD bug reports: `.rotate(0, 0, 45)` fails with *"If a newer parcad wrote the
part, this host (parcad 0.0.9) is too old for it: update it"* when the real
mistake is the argument form; and `parcad call --set` sends numbers as strings,
so `timeout_s=120` is rejected with `invalid type: string "120", expected f64`.
Both went nowhere -- the user did not relay them, and they exist only because
someone read this transcript. A `report_issue({ tool, called, happened,
expected })` appending to `.issues/<date>.jsonl` with the host version, read by
`parcad issues`, is ~150 lines and manufactures exactly the evidence
`field/` exists to stage -- from real sessions instead of written ones. The
field harness measures whether a model *can* read a tool; this measures where
one *did not*.

**The `--set` bug itself.** Three failures in one session: `threshold_mm: "1.0"`
over MCP, `timeout_s=120` twice over the CLI. Parse `--set` values as JSON
first in `crates/parcad-cli/src/call.rs`; the schema stays strict. ~20 lines.
It cost this session two round trips and a Python traceback.

**`section: { plane: "x" }`** failed with *"failed to deserialize parameters:
missing field `axis`"* -- a serde message, not a ParCAD error, naming neither the
field the caller used nor the fix. Every tool whose arguments are a struct
should catch the deserialize error and answer in ParCAD's own voice: *"a section
takes `axis` ('x', 'y' or 'z'), not `plane`."* WRITING_FOR_MODELS rule 7.

**A viewer nobody is at still counts as shown.** Turn 1's `open_project` reply
listed a browser viewer last heard from **6942 seconds** ago, still drawing the
bracket at 55074.791 mm3. Turn 2's listed the desktop window -- the one the user
watches -- at revision 2, **800 seconds** stale, while the browser reported the
new revision. `session::wait_until_shown` returns as soon as
`any(|v| v.revision >= revision)` (`session.rs:237`), so the call was satisfied
by a tab abandoned two hours earlier, and the reply said the part was on screen.
Mark a viewer that has not reported in five minutes `stale: true`, wait for
every non-stale viewer rather than any viewer, and have `open_project`'s
description say to name which window. ~60 lines.

---

## Ranking

Turns saved for the user, per hour of work. "Turns" counts the six user messages
this session actually spent.

| # | proposal | turns it removes | days | first? |
|---|---|---|---|---|
| 1 | **The brief** (+ the render's scale rule) | 2 -- turn 2 and turn 6 are both requirement corrections | 1.5 | **yes** |
| 3 | Overhang measured + print view | 1 -- turn 3's first half | 1.0 | |
| 4 | Print on save + the window's button | 0 turns, but it is the difference between a session that ends and one that ships | 1.5 | |
| 7 | `project` + `edits` instead of resends | 0 turns, ~15-20 minutes | 0.5 | |
| 6.1 | `list_snapshots` reports each version's size | ~0.5 -- "go back to the one you liked" becomes sayable | 0.25 | |
| 2 | Checks that live in the part | 0 turns, 8 builds, and the end of "not yet re-run" | 2.0 | |
| 5 | `compare_variants` | ~1.5 across turns 2/4/6, but wants 1 and 3 first | 1.0 | |
| 8 | Styles listed, gestures, no fake hardware | ~0.5 of turn 4 and the worst of turn 6 | 1.0 | |
| 6.2-3 | Version strip, fork-don't-overwrite | ~0.5 | 0.75 | |
| 9.3 | Exploded view | ~0.5 of turn 3 and turn 5 | 0.5 | |
| 10 | `report_issue`, `--set` JSON, serde errors, stale viewers | 0 now, compounding | 0.5 | |

**Build the brief first.** It is the cheapest fix for the two user turns that
were pure requirement failure, it is the column that makes section 5's comparison
table mean anything, it costs a day and a half, and -- unlike every other item
here -- it changes what the *first* build of a session reports, which is the only
place in a conversation where a correction is still free. Ship the render's scale
rule in the same session: the brief is the fact, and the rule is that fact in the
channel the user actually looks at.

---

## 11. Two field cases, written before the thing is built

Both need a seed part added to `examples/` and their numbers pinned from the
first build -- `verdict`, `trap` and `quote` below carry the values I expect and
must be replaced with measured ones before the case is committed, per
`eval/cases/`' rule that a case green on a wrong recorded number is worse than
no case.

### `eval/field/does-it-meet-its-brief.md` -- for section 1

```markdown
---
tool:    evaluate_part.brief
also:    read_project
reach:   evaluate_part
verdict: OVER\s*[=:]\s*1[45]\.[0-9]\s*mm\s+along\s+x
trap:    OVER\s*[=:]\s*(0|none|4\.0)\b|FITS
quote:   \b1[45]\.[0-9]\b
why: |
  `pocket-tin.js` declares brief({ envelope: [95, 70, 16] }) and the script's
  own outline variables say plateW = 99 -- 4 mm over, which is the plausible
  wrong answer sitting one line from the right one. The built part is wider
  than plateW: a 1.6 mm skirt and a 1.5 mm corner fillet put the measured
  bound at 110.0, so the overrun is 15.0 mm and not 4.0. No render settles it
  (the part floats on black at unknown scale) and `size` alone does not either
  -- the case is whether the brief verdict is read, which means reading it
  against the envelope in the script rather than subtracting two numbers the
  script hands you. The envelope also admits any of six axis orientations, so
  a model comparing [95, 70, 16] to [110.0, 56.9, 10.7] term by term gets the
  wrong axis.
---
Use the parcad MCP tools. The part is `pocket-tin` in the parcad project folder.

Question: this part was designed to fit inside a stated envelope. Does it, and
if not, by how much does it miss and along which axis?

Rules: every number in your answer must come from a parcad measurement, not
from arithmetic on the script's variables -- if you catch yourself computing it
from plateW, skirtT or cornerR, that does not count and you must go measure it
instead. End with a one-line verdict in exactly this form:

OVER = <millimetres> mm along <x|y|z>
```

### `eval/field/where-does-it-need-support.md` -- for section 3

```markdown
---
tool:    evaluate_part.overhangs
reach:   evaluate_part
verdict: UNSUPPORTED\s*[=:]\s*2[01][0-9](\.[0-9])?\s*mm2?\s+(at|on|in)\s+`?lip`?
trap:    UNSUPPORTED\s*[=:]\s*0\b|(?i)\bno (unsupported|overhang)
quote:   \b2[01][0-9]\b
why: |
  The part prints flat, is watertight, stands on one patch over 74% of its
  footprint and fits every bed -- every printability number parcad had before
  this feature is green on it, which is the point. Its eight retaining lips
  each reach 1.5 mm straight inward at 0 degrees, unsupported, and nothing in
  the source names an angle. A render does not settle it: the lips are inside
  the cups and an iso view shows their top surface, not their underside; a
  section shows two of eight. The trap is the answer every earlier version of
  this reply supported -- that a part which sits flat and needs no raft has no
  overhangs. `lip` is a tag, so the second half of the verdict is also a test
  of whether the places list is read rather than a total quoted.

  Written before the feature exists. Pin the mm2 from the first build and
  narrow the verdict regex to it.
---
Use the parcad MCP tools. The part is `coin-cups` in the parcad project folder.

I am printing this on an FDM printer with no support material, as drawn.

Question: how much of this part's surface is unsupported, and which named
feature is it?

Rules: the number must come from a parcad measurement, not from counting
features in the source and not from reading it off a picture. If anything you
measured was not the finished part, say so. End with a one-line verdict in
exactly this form:

UNSUPPORTED = <number> mm2 at <tag>
```

**Two more worth writing once the features exist**, sketched rather than
written: an authoring case for section 7 (after "make the top plate 1.2 mm", does
the call carry `edits` or a 200-line `script` -- scored by `input`, the only way
to score an authoring case); and a section 2 case where a part's own declared
check is red and the question is "is this ready to print" -- SOUND only when the
reply names the failing check and both bodies, in both arms.

---

## A closing note on the thing that is not a tool

The session's most expensive single failure is not in this list because it is
not a feature. In turn 2 the model wrote *"The pictures below have 2 coins in
each cup"* about a grey-on-grey render, while the ParCAD window in front of the
user showed the same part with no coins in it at all. Two channels, one
sentence, and the user had no way to tell they disagreed. Everything in section 9
is an attempt to make that impossible rather than to make it less likely -- and
the reason `open_project`'s `viewers` list is worth fixing (section 10) is the
same: the user's screen is a measured fact, and ParCAD is the only tool in the
room that can measure it.

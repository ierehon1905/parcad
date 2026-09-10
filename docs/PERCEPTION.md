# What an agent can see, and what it should be told instead

Notes for maintainers and agents working on the perception surface — not an
introduction to it.

parcad's MCP surface exists so a model that cannot open the window can still
work on a part. The counterpart of [OP_ROADMAP.md](OP_ROADMAP.md): that one asks
which *operations* to add, this one which *perceptions*.

> **Anything an agent could answer by squinting at a picture gets answered with
> a measured number instead.** A render is for shape, presence and "is this the
> part I asked for". A number is for dimension, count, thickness and clearance.
> The two ship in the same reply, and where they disagree the number is right.

That is "report measured values, not requested ones" pointed at the *reader*
rather than the author, and it is why several entries below are *hold* — a
cheaper tool already answers what the expensive one was for.

---

## What the models actually do

**Good at** overall shape, presence or absence of a requested feature, gross
topology, obvious symmetry breaks. **Bad at** metric dimensions read off pixels,
small gaps and failed booleans, thin-wall violations, counting past about six,
mental rotation between two views. [CADSmith][cadsmith] is the single most
useful data point: a quadcopter frame scored IoU 0.985 and passed every vision
check by a Claude Opus judge while containing gaps between the arms and the hub
that "three fixed views cannot resolve". The failure that reaches a machine shop
is the failure a render hides.

- **Three orthographic views plus an isometric is the ceiling, not a floor.**
  [OpenECAD][openecad] measured *four* views performing **worse** than fewer.
  Our seven-panel sheet is a default worth revisiting; §2.
- **Orthographic beats perspective for anything dimensional.** [OG-VLA][ogvla]
  renders orthographic to remove the ambiguity; [Ortho2CAD][ortho2cad] and
  [CReFT-CAD][creft] work from front/top/right with dashed hidden edges and
  burnt-in dimensions.
- **Overlaid identifiers unlock grounding.** [Set-of-Mark][som] turns "the hole
  on the left" into "face 7". `tags.rs` does the colour version; §4 is the rest.
- **A depth channel improves size and distance perception**
  ([SpatialVLM][spatialvlm], [DepthLM][depthlm]) — for photographs, where depth
  is estimated. We have the buffer exactly, so it is a probe, not an image; §12.
- **Letting the model render its own diagnostic beats more of ours.**
  [Whiteboard-of-Thought][wot] reached 92% where chain-of-thought scored 0%,
  purely by letting the model write plotting code and look at it.
- **A text grid is the worst of both.** [ViTC][vitc] measured GPT-4 at **25.2%**
  recognising a single character rendered as ASCII art and **3.3%** on two- to
  four-character strings, chain-of-thought barely moving it;
  [ASCIIEval][asciieval] and [ASCIIBench][asciibench] replicate it. §10.

---

## Where we stand

| capability | parcad | note |
|---|---|---|
| Numbers and pictures in one reply | ✅ `evaluate_part` | `EvaluationSnapshot` as text *and* structured content, then the PNGs — looking costs no extra call |
| Measured dimensions, volume, area | ✅ `PartReport`, `measure.rs` | tight `bounds`, never `framing_bounds` |
| Multi-view contact sheet | ✅ `render::contact_sheet` | seven orthographic views, one shared framing |
| Scale bar on every panel | ✅ `render::ScaleBar` | round 1-2-5 lengths, end ticks |
| Region colouring with a legend | ✅ `tags.rs` | which tag owns which surface; key beside the frame, colours hashed from the name so two renders stay comparable |
| Where each tag is | ✅ `tags::extents`, `evaluate_part`'s `tag_extents` | §3 — one box and one centre per tag, from the built surface |
| Which way a view looks | ✅ `RenderedView`'s `axes` | §2 — view names are absolute, and saying so found two of them mirrored |
| Edge listing with geometry | ✅ `list_entities` | centre, direction, length; sampled at 60, with the total — and it disagrees with `evaluate_part`'s `topological_edges`, which double-counts. §13 |
| Treatment target preview | ✅ `inspect_treatment_target` | plus tags whose edge set is *exactly* the target |
| Selector syntax check | ✅ `check_selector` | no geometry touched |
| Depth + normal per pixel | ~ `render::GeometryBuffer` | exists, and `model_point` ties a pixel to a millimetre — not exposed |
| Point and ray probe | ✅ `probe.rs`, `probe_part` | §3 — signed distance at a point, every crossing along a ray, the wall thickness between them |
| Wall thickness / minimum feature | ✅ `thickness.rs`, `measure_wall_thickness` | §5 — a ray from every sampled surface point, both faces named; optimistic where a fillet was dropped, and it says so |
| **Overhang and printability** | ❌ | §6 |
| Section view | ✅ `render.rs`, `evaluate_part`'s `section` | §7 — a clipping plane in both renderers, the cut face capped and drawn flat, and `cut_fraction` to say whether it opened anything |
| **Numbered marks on the render** | ❌ | §4 |
| **Diff render** | ❌ | §8 |
| **Face adjacency as text** | ✅ `Shape_faces_json`, `list_entities` | §9 — kind, exact area, centroid, normal and neighbours; 16/16 SOUND on `does-the-blend-reach-the-bolts` |
| Adaptive slice summary | ❌ | §10, the salvaged form |
| ASCII / voxel dump | ❌ | §10, hold, with numbers |

---

## 1. The shape of a perception tool

1. **One call answers the question.** A probe that returns a distance but not
   the point it hit forces the second call the first was supposed to prevent.
2. **The answer names a location.** "Minimum wall is 0.8 mm" is half a tool.
   "Minimum wall is 0.8 mm at (12, −4, 20), between the bore and the outer
   face" is one — the standard `OcctError::Crashed` sets by naming the fix.
3. **Nothing is inferred that could be measured.** This is the whole page.

### How to tell whether one works — run the field suite

**A perception tool is not finished when its number is right. It is finished
when a model reads the number right, and those are different days' work.** §3's
probe passed its Rust tests on the first run and then failed three ways in front
of a model — a flag read inverted, a field name read as the wrong noun, the tool
not called at all — and a fourth from the other end: the server's own
instructions named a field (`rendered_by`) that no reply has ever contained.
None of the four is reachable from inside the process.

```bash
mkdir -p /tmp/parcad-field-projects
cargo build -p parcad-app --bin parcad-app     # the build you mean to test
PARCAD_PROJECTS_DIR=/tmp/parcad-field-projects PARCAD_HTTP_PORT=4344 \
  PARCAD_OCCT_WORKER=$PWD/target/release/parcad-occt-worker \
  ./target/debug/parcad-app &
PARCAD_HTTP_PORT=4344 field/run-suite.sh 3          # every case, both arms
PARCAD_HTTP_PORT=4344 field/run-case.sh eval/field/does-the-port-meet.md 4
```

Each trial is `claude -p` on Haiku 4.5 — a separate process, its own context,
every local tool denied so it cannot open the file and read the answer — against
the MCP server the running app hosts.

**A trial is graded, not passed.** SOUND is the right answer reached by the
route the case requires; LUCKY is the right answer without it. Adding the two is
the failure this page keeps re-learning: §3 round 1 scored 3/4 *correct* while
measuring almost nothing, one trial quoting the part's own source comment as its
proof. Read the per-case `reach` column first; `field/score.py --show` prints a
transcript as prose. Five things this cost to learn:

- **The app serves the binary it started with.** Rebuild *and restart* between
  rounds; a round that tested the old build looks like a round where the change
  did nothing.
- **Trials are parallel and independent**, so four at once take one trial's wall
  clock.
- **Extended thinking is on unless you turn it off,** so an unconfigured run
  measures the reasoning model only.
- **Read the transcript, not the verdict.**
- **A tool missing from the runner's allow list is invisible, not failed.**
  `save_project` and `export_part` sat outside it from the beginning, which is
  why nothing had measured whether a model can put its work where the user will
  find it. The deny list is wrong by default every time the CLI grows a built-in;
  the *allow* list every time this project grows a tool, and it fails in the
  direction that looks like a result.

`eval/field/README.md` has what makes a case worth adding. Write the result into
the section it bears on, beside the decision it changed.

## 2. Fewer views, chosen — a correction to the default

The contact sheet ships all seven of `View::ALL`: right for a person, probably
wrong for a model, given [OpenECAD][openecad]'s four-below-three result — back,
left and bottom are usually redundant with their opposites on a roughly convex
part. Nothing in the kernel changes, `parse_views` already takes a list; only
the *default*: `iso, front, top, right` for an agent, all seven on request, with
the tool description saying which to ask for and why. It is a claim about our
renders, not theirs, and an eval at four views and seven would settle it.

**A view name is absolute, and now says so — done.** `front` looks along +Y and
shows the XZ plane whichever way the part faces, so a session modelling a car
whose length ran along X got its *side* elevation under the name `front` and
misread two rounds of images before it clicked. Every `RenderedView` now carries
`axes` — `looks_along`, `up`, `right` as model unit vectors, plus "looks along
+y and shows the xz plane, with +x right and +z up", off the same matrix so the
sentence cannot drift from the camera.

**Writing the axes down found that two of the views were mirrored.** The screen
basis for `left` and `right` had determinant −1 — a reflection, not a rotation —
so a boss standing off a part's +Y face drew at column 94 of 128 in the `left`
view where it belongs at 34. Nothing had noticed, because a mirrored picture of
a symmetric part is the same picture and every part in `examples/` is symmetric
about at least one of these planes; an agent reading a side view of a *handed*
part got the handedness backwards. docs/GOTCHAS.md has the fix and the pixels.

## 3. Point and ray probes — **DONE**

`crates/parcad-core/src/probe.rs`, reached as `probe_part`. `distance_at(points)`
returns the signed distance at each — the sign alone answers "is this point
inside the part", which previously needed a render and a guess.
`ray(origin, direction, max)` returns every crossing along the line, in order,
plus `solid_mm` and `first_solid_mm`. Two crossings on one ray *is* a wall
thickness, measured; [CADSmith][cadsmith]'s gap-at-the-joint failure is one ray
cast.

- **Bisect on the sign, not on the distance.** The field is exact for primitives
  and cheap booleans and an **under**-estimate at corners by construction
  (OP_ROADMAP §1 — an overestimate deletes geometry, because the octree prunes
  on it). A sphere trace on such a field converges and never lands, so the march
  floors its step at `EPS` to let a sign change happen, then bisects on the
  sign: exact where the magnitude is not, so thicknesses are right even where
  the field around them is conservative. A distance at a point stays a lower
  bound, and says so.
- **A ray that grazes a surface can march forever**, reading a near-zero
  distance and advancing `EPS` a step — real geometry, not a bug, so it is
  capped and reported (`incomplete`). Past the last crossing the answer is
  *unknown*, not *nothing there*.
- **The field has no fillets, and the report has to say so.** `drawable()`
  replaces every `Fillet` and `Chamfer` with an identity so a part can be drawn
  at all; a probe goes through the same door and so measures the **sharp**
  corner. `omitted_treatments` names every treatment dropped.
- **No length means "all the way through".** The default reach comes from the
  origin and the framing bounds and is reported as `max_distance_mm`.

**Measured, not assumed.** Unit tests pin closed forms: a 40 mm cube shelled to
5 mm reads a 5.000 mm wall and 10 mm across two walls; service tests use the
40 mm plate with a Ø12 bore, wall beside the bore 14 mm. On generated geometry —
`examples/hex-standoff.js`, 5.5 across the flats, 2.5 tap drill — a ray across a
flat crosses at ±2.75 and ±1.25 and reports `first_solid_mm` 1.4999993 against a
closed form of exactly 1.5, a ray down the bore finds nothing, and the point at
the origin reads +1.25 from the bore wall.

**What a model does with it is the separate fact.** Asked for that same wall
with `probe_part` withheld, Haiku 4.5 produced the same 1.5 mm — *derived* from
`acrossFlats` and `tapDrill` in the script, in the language of measurement, at
"99% confidence". Not an unreachable number but a computed one wearing a
measurement's clothes, and where source and built geometry have diverged that
derivation is confidently wrong with nothing in the answer saying so.

**A probe says whether there is material, not what it is in, and it took three
rounds of naming before a model read that.** Asked to prove `manifold-block.js`'s
drop ports meet the main gallery, a model measured correctly — port void z=20 to
z=−5, gallery z=+4 to z=−4 — and concluded they *did not* meet, inventing a
millimetre of material between −4 and −5 its own ray had measured as void: two
overlapping intervals read as two adjacent ones. Then, in order:

- **`inside` was read inverted.** Handed `{"distance_mm": 5, "inside": false}` a
  trial wrote "the material is solid with 5 mm of solid material remaining", and
  a negative distance a millimetre lower as "inside a void". Inside *what* is
  ambiguous on a part made of negative space, and a boolean gives a model a coin
  to flip; `medium: "material" | "void"` is the same information with no free
  parameter, and no trial in either arm has misread it since. Same change for
  `starts_inside`, `ends_inside`, `entering`.
- **`tag` was read as the far side.** Handed a crossing carrying
  `{"into": "material", "tag": "ports"}`, a model wrote "crosses into **port
  material**" — the tag taken for the stuff beyond the crossing rather than the
  face it went through. The field is `surface_of` for that reason. Its first
  round also settled that a field nothing reads is not a feature: the tag was
  correct, and in front of the one trial that could have used it.
- **`probe_part` was not being reached** — 1 of 4 in the first round, and no work
  inside the tool fixes that. Its description now says *this is the tool for "do
  these two bores meet", reach for it before you reason from a dimension in the
  source*. Over nineteen trials in three rounds, half with extended thinking off,
  reach went 1/4 → **8/8** and correctness to **7/8**. **The most valuable change
  to a perception tool was not in the tool.**

**A field name read as the wrong noun is the recurring failure here, and none of
it is reachable from inside the process.** Every one of these appeared with
thinking *on*, and the last round's only wrong answer is from the reasoning arm
while its non-reasoning arm went 4/4. A right answer is not evidence either —
`field/score.py` prints who *reached* the tool for that reason, and had a bug
that scored "DO NOT MEET" as a pass. That wrong answer measured 13 mm between
the gallery and a port and was right, at z = 16, having decided that was where
the gallery was; it is at 0. A model cannot aim a ray at a feature it cannot
locate, and down a port's axis the port void and the gallery void are the same
air — the decisive measurement is transverse, at the gallery's own height, and
no trial fired one.

**No `eval/cases/` entry, deliberately.** A case there is a two-backend geometry
comparison — `Observed` is size, volume, area, triangles, topology — and a probe
is neither a geometry nor available on both backends. §5 landed and the answer
is still no: a thickness is implicit-only for the same reason, and the B-rep
backend has none to disagree with. Closed forms are pinned in the unit tests
instead — for `thickness.rs`, a 40 mm shell, an off-centre pocket with a 2 mm
wall on one side and 12 mm on the other, and a sphere.

### Where is this tag? — done, and it answers a failure class

A detailed 1:10 model car passed every automated check this project has —
watertight, manifold, not one wall below the print threshold after three rounds
of `measure_wall_thickness` fixes, every `.expect({ count })` matching — and was
**wrong as an object**: the cabin faced the opposite way from the body. Seven
rendered viewpoints did not show it; a person glancing at one caught it in a
second. `tag_extents` on `evaluate_part` is that bug as a number, same call:

```
lower  x -218.000 .. 218.000   centre    0.000
cabin  x -115.000 .. 100.000   centre   -7.500
```

`tags::extents` bounds the points of the *built* surface whose own field
vanishes on each tag, so a feature the kernel did not build has no extent and
says so in `unlocated_tags`. Four things the implementation settled:

- **An extent is inclusive where a colour is exclusive.** `owners_at` gives a
  point to the nearest tag, because a pixel takes one colour. Under that rule
  `examples/flange.js` reported its bore 11.95 mm deep in a part it runs
  23.9 mm through: both bounding rims are points where two tagged surfaces
  genuinely meet, and both were won by the face. Nested tags now report nested
  boxes.
- **The vertex list is not a sample of the surface.** An exact kernel meshes a
  cylindrical face as two rings of nodes and nothing between — the chordal error
  is entirely circumferential — and both rings are rims, so before
  `surface_sample` added every triangle edge's midpoint, `bore` came back as a
  flat ring at a single z. Midpoints *not* on the surface fix themselves: a
  chord's midpoint sits inside the material by more than the attribution
  tolerance and is claimed by nothing.
- **The error is two-sided and bounded by the mesh.** On the car, the exact
  backend reports −115.000..100.000 to the micron; the implicit one at depth 7,
  resolution 3.611 mm, reports −113.842..98.262 — short by 1.2 and 1.7 mm — and
  `lower` reads ±91.168 against an authored ±90, over by 1.2. Short because the
  extreme point of a surface is rarely a sampled one, long because a point
  within tolerance counts as on it.
- **A tag names a node, not a placement.** `cylinder(...).at(12, 0, 0).tag()`
  tags the translation and reports the hole where it is; `cylinder(...).tag()`
  used later at `.at(12, 0, 0)` tags the primitive, and its extent is the
  primitive's own surface at the origin.

**Measured on a model, and it is read.** `eval/field/where-is-the-feature.md`
asks Haiku 4.5 which of the flange's tags names a feature entirely above the
mid-plane and at what z its surface begins. `hub` is authored as a cylinder
placed at `hubTop / 2`, so the *script* says z = 0, and what the kernel built
starts at 12.38 because a 3 mm blend replaced the bottom of it: a trial that
measured says 12.38, one that derived says 0.

| arm | trials | reached `evaluate_part` | quoted the measured z | SOUND |
|---|---|---|---|---|
| thinking off | 2 | 2/2 | 2/2 | 1 (+1 LUCKY) |
| thinking on | 2 | 2/2 | 2/2 | 2 |

The winning trial is the argument for the whole page: four `ToolSearch` calls,
`read_project`, one `evaluate_part`, and *"Perfect! I can see the tag extents
clearly"* — no probe, no `list_entities`, no second call. **The round before it
is the more useful one, and it was void:** four trials in the reasoning arm, app
died partway, every trial VOID on `unable to connect`. What they did first still
counts — two reached `evaluate_part`, *none* quoted a tag extent, and the one
with the reply in front of it went hunting instead: three rounds of
`probe_part`, a `list_entities`, eleven calls, and the wrong answer, `HUB starts
at 15.85`. Four clean trials do not prove that is gone. The region map was
separately re-run after its legend and palette changed — `what-is-hidden`, 2/2
SOUND, still reading `visible: false` and the tag missing from `regions`.

## 4. Numbered marks — the rest of Set-of-Mark

`tags.rs` colours a render by which tag owns each surface and draws a legend:
Set-of-Mark with colours as the identifier. The literature's version uses
**alphanumerics**, and the difference is not cosmetic — a model can say "7" in a
selector and cannot say "the olive one". See `7` on a face, read face 7's
metrics in the same reply, write a selector that resolves to face 7;
`inspect_treatment_target` does this for edges *after* the fact, marks do it
before.

**What it takes.** `render.rs` has `text` and `label`, and
`GeometryBuffer::model_point` turns a pixel back into a point on the part. The
work is placing a label so it lands on the region it names and does not collide
— the centroid of the region's pixel mask, nudged inward. A mark is a rendering
artefact valid for one evaluation, exactly like `edge@N`: it must never look
like something to write in a script, and the tool description has to say so in
the same words `list_entities` does.

**The legend is out of the frame, and the colours no longer reshuffle.** The
legend was drawn down the left edge *over the part*: at 640 px it covered
roughly the left third and the top half, and one session's car had its nose at
the left, so the overlay hid the region carrying the orientation cue. It is now
a strip beside the frame — the part keeps pixel (0, 0) and the framing it would
have had with no legend — sized from every tag rather than the visible ones, so
the part sits at the same pixels in all seven views. Dropping the key is worse:
the reply names a colour as `#e85d4e`, which no reader matches to pixels by eye.

Colours are hashed from the tag *name* rather than handed out by position,
because the working method here is comparing a render against the previous one
and a palette that reshuffles when a tag is inserted early in a script destroys
that comparison silently. Two tags sharing a colour would be worse than the
shuffle, so the lower hash keeps the slot it wanted and the other walks to the
first free one. Hashing also lets any *pair* of palette entries meet, where
positional assignment only ever used a prefix — and the palette did not hold up
past the front: blue and periwinkle sat at ΔE 13.9, and a trial separately
reported red and salmon at 20.7 as indistinguishable across a wheel arch. Four
entries were replaced; minimum pairwise separation is now **34.5**, pinned by
`no_two_palette_entries_look_alike`.

Authored colour — `.tag(name, { color })`, and a per-render `colors:` override —
was asked for alongside it and is **not** built. `tag` is semantic: it names
what a thing *is*, and that name does real work in `probe_part` and
`measure_wall_thickness` output, where colour would be the first purely
presentational thing in the language. The diagnostic case — *put this one
feature in screaming magenta and everything else grey* — is the half worth
revisiting first if it comes back.

**The cheaper half is done: a `Crossing` names what it is on.** `tags::owners_at`
asks of three coordinates what `regions_in` asks of a pixel, so every crossing
carries the `tag` of the node whose surface it is, or nothing where an untagged
node or a fillet owns it. `crossings_tell_two_voids_that_meet_apart` is §3's
manifold, measured: across the part at the gallery's height the void is bounded
by `port` on both sides, which *is* the intersection — the same fact a model got
backwards as a pair of diameters. Two surfaces can genuinely meet at a point, a
bore's wall and the face it breaks out of at the rim, and there the nearer wins;
that ambiguity is real rather than a rounding choice.

**Cost.** Medium, for the marks that remain — still the highest-value *visual*
change.

## 5. Wall thickness and minimum feature

The smallest amount of material anywhere, and where it is; the check every part
in `examples/` silently assumes and none verify. **Done**, as `thickness.rs` and
`measure_wall_thickness`: minimum, the point, the two surfaces it lies between —
named through `tags::owners_at` — and a count of samples below a caller-supplied
threshold, so "one bad spot" and "the whole wall is thin" are distinguishable.
Surface points and normals come free from the `GeometryBuffer`, one per hit
pixel across all seven views, and the loop is `probe::rays`: compiling the field
once per ray is fine for a handful and ruinous for thousands.

- **The omission is stated as prose, not as a list.** `omitted_treatments`
  carries node indices as everywhere else, and a `caveat` string beside it says
  *which way the error runs*: the sharp corner has more material, so the
  reported minimum is an upper bound. A list of indices makes an answer vaguer;
  only this one makes it optimistic, and the field name cannot say so. Sampling
  the B-rep surface instead is still the real fix, and is still not done.
- **Surface samples need refining before they are surface samples.**
  `model_point` reads back a *quantised* depth, landing within a voxel of the
  surface — far enough out that the inward ray starts in void, or far enough in
  that every wall reads short by the same bias. Two Newton steps along the
  gradient close it. But a field built from `abs` or `sqrt` has no derivative
  where it is exactly zero, so a point landed perfectly on the surface returns
  `NaN`: success and failure look identical. The last *usable* normal is kept,
  never the last one evaluated.
- **A ray thickness is not an inscribed sphere**, and the difference is signed:
  they agree on a wall with parallel faces, and in a concave corner the ray
  crosses to whatever is straight across, further than the sphere that fits.
  Upper bound again, stated in the module rather than discovered later.

**Measured on a model**, `eval/field/how-thin-is-it.md`, four trials of Haiku
4.5 with thinking on, against the flange: *what is the thinnest material in this
part, and between which two surfaces?* The ligament between a bolt hole and the
OD is 6.3 mm, and `thickness = 19.1` sits in the script one line away from being
the wrong answer. 4/4 reached the tool, 4/4 quoted a measured value, 4/4 correct,
and 4/4 reproduced the caveat *with its direction* — "the true minimum is at or
below 6.30 mm". First time a warning in a payload has been read back correctly
on the first round; the tool description's *use this before you call a part
ready to print* is the likeliest reason, as in §3 round 3.

**The transcripts say the tag names are not enough**, which the score does not.
All four named the pair `plate` / `drilled` and then explained it in English, two
of them wrongly — one put the wall between "the top surface of the flange" and
the bolt holes. It is neither: the wall is *radial*, OD to bolt hole, and both
trials had the coordinates that say so (`at` and `opposite` share a z). A tag
names a *node*, not a face:
`plate` is one cylinder owning the OD, the top and the bottom, `drilled` one cut
owning the bore and all four bolt holes, so `surface_of` narrows to a handful of
faces and stops and the model fills the rest in from the part it is imagining.
An argument for §9 rather than a defect here.

**The round after it was void, and that is worth recording too.** Rerunning
`does-the-port-meet.md`, two of four trials never answered: stuck, they went
hunting for a shell, found `Monitor` and `Skill` — neither on
`field/run-case.sh`'s deny list, which predates them existing — and spent the
run trying to fix this repo's compiler warnings, and the scorer counted them as
trials. A deny list is wrong by default every time the CLI grows a tool, so the
scorer now prints a `stray` column instead of trusting it; eval/field's README
has both failure modes. The §5 round above is unaffected — all four of its
trials called nothing but `ToolSearch` and parcad.

**Replies are rounded to the micron**, in `service::round_mm` and nowhere else.
Not a size optimisation first — though it is a quarter to a third of every
numeric reply — but this page's rule pointed at precision. The field is
evaluated in f32, and an f32 widened to f64 has no short decimal form: 30.15
serialises as `30.149999618530273`, because `serde_json` must print enough
digits to round-trip the f64 it was handed. Those fourteen trailing digits are
the f32's own rounding error presented as measurement, and a centroid of
`6.066550368146516e-7` is a zero that reads as an offset; a model has been
observed spending tokens reconciling the noise — *6.30 mm (measured as
6.29999268054804 mm)*. Re-run against `does-the-port-meet.md` afterwards, four
trials of Haiku 4.5: 4/4 reached the tool, 4/4 quoted a measured value, 4/4
correct, none strayed.

**What the flange round could not show is that the minimum is sometimes not a
wall.** Swept over every part in `examples/`, it answers sanely for nineteen and
returns 0.0055 mm for `hydraulic-line.js` and 0.0069 mm for `timing-pulley.js`.
Both are correct parts, and the two failures differ:

- **A tie in a `max` hands back the wrong surface's normal.** Where a bend's arc
  is trimmed by its own end plane, the torus's field and the plane's are both
  zero, so the gradient comes back as the plane's while the surface there is
  vertical. The ray runs *along* the face and finds f32 noise — the same seam
  reads 0.0055 mm at 96 px and 0.0096 mm at 256 px. `GRADIENT_TOLERANCE` is
  meant to be this guard and cannot be: the wrong branch of a `max` still has a
  unit gradient. Re-measuring at a second resolution is the cheap discriminator,
  since a real feature reports twice.
- **A tangential feature genuinely has no minimum.** The pulley number is this
  kind, and so are the 0.21–0.24 mm spots on the same hydraulic line, which are
  exact: the ring between the inlet boss's OD and its O-ring groove is
  `0.5 − √(1 − (x − 4)²)` mm thick and tapers to zero at the groove rim. Sample
  nearer the rim, get a smaller number, without limit. Every groove and every
  run-off blend does this, and no sampling improvement touches it — "the
  thinnest material anywhere" is a different question from "is there a wall here
  too thin to make", and this tool answers the first.

That is why the minimum is *not* in the `evaluate_part` reply, where it would be
a free thin-wall warning on every edit: two false alarms in twenty-one shipped
parts teaches a reader to skip the line. docs/GOTCHAS.md, "The cut that seals a
void is refused", carries the measurements — and the part of that defect which
*could* be caught, once restated as topology rather than as a thickness, now
refuses at the kernel instead of being reported here. A perception this page
cannot make trustworthy is sometimes a refusal the kernel can make exact.

## 6. Overhang and printability

A per-face angle test against the build direction. [AgentsCAD][agentscad] uses
the deterministic rule you would write by hand — flag any face whose normal
makes too shallow an angle with the build plane — plus radius of gyration as a
footprint proxy and an elongation index for how lopsided it is. Same argument as
`METRIC_FASTENERS`, and the natural home for the thread annotation OP_ROADMAP §5
wants: a report that knows a hole is tapped M6 can check the boss around it is
thick enough. Small on the B-rep side; `mass_properties` in `measure.rs` gives
the inertia terms. **Report the geometry, not the verdict**: "face 12 overhangs
at 18° from the build plane" is a measurement, "this part will fail to print" is
a process opinion about a machine we know nothing about.

**Done, the first piece: what the part stands on.** `stands_on` in every report
and snapshot — the surface lying in the part's lowest plane, in mm², how many
separate patches, and that area over the bounding footprint. It exists because
of a part that passed everything else: a plate stand whose pegs were placed on
the underside plane instead of the top measured a plausible 45 mm tall, was
watertight, had the right volume to within a percent, and stood on eighteen
stubs of 130 mm² each. Its line reads `on 2320 mm² at z −5.53, 18 patches, 7% of
the footprint`; the corrected part's reads `on 26469 mm² at z 0.00, 1 patch,
74%`. The tolerance for "in the plane" is the mesh's own resolution, since a
dual-contoured plane scatters its vertices by up to a cell. Recorded for every
measured case in the corpus and shown on the app's report panel, in red under a
tenth of the footprint. Still a measurement, not a verdict: a stool stands on
four patches and is fine.

## 7. Section view — done

Filed on OP_ROADMAP as §8, a view concern rather than an op, and the single
thing most missed while writing the corpus (DSL_GAPS §0). For an agent a section
is the only way to see an internal feature at all, and `intersect(part, box(...))`
is the wrong answer because it produces a *different part*. `evaluate_part` takes
a `section` and nothing about the part changes: it is how the picture is drawn.

**One `Section`, two renderers.** `view::Section` is an axis, a position and a
side, and both defaults resolve *per view*, because the half that has to go
depends on where you are looking from. The raymarcher gets this almost free:
intersecting the field with a half-space is exact and the cut arrives as
ordinary surface. The rasteriser, which is what an agent's renders come off, has
to cap the hole the clip leaves or a solid boss draws as a thin cup.

**Capping is a parity count, and the textbook answer was wrong here.** Each
pixel counts the crossings the clip threw away; odd means the ray was still in
material at the plane, so that pixel is cut face. The signed-winding version —
+1 for a face turned toward the viewer, −1 for one turned away — is the standard
and needs each crossing's *facing*, which means the mesh's normals, which dual
contouring does not have at a sharp feature: on a plain cube it reports the top
face's normal along a vertical edge, and a third of the part comes back falsely
capped. Parity needs no normals. Both renderers now agree pixel for pixel, which
is the test that caught it.

**What is reported, because the failure is silent.** A plane clear of the
material, or one this view looks *along*, produces a perfectly ordinary picture
of an uncut part. So every sectioned view reports the plane it actually cut —
`at_mm` and `keep` resolved, whether the caller named them or not — and
`cut_fraction`, the share of the drawn part that is cut face; zero means you are
looking at an uncut part. The cut face is drawn flat, in a colour no lighting of
the grey material can produce. A cut face is not surface of the part, so
`tags.rs` skips those pixels: counted, every one would come back unattributed
and a sectioned region map would report a well-tagged part as mostly unclaimed.

**The window got it from the same change.** The viewport is three.js and shares
no code with the raster, only the same three fields, the same rule for which
half to keep, the same cut colour, and the same parity count, done by the GPU in
its stencil buffer while drawing the part's own back and front faces. Two things
there fail silently: three.js compiles the *number* of clipping planes into the
shader, so a material that already has a program ignores a plane added later,
and `stencil` has defaulted to false on `WebGLRenderer` since r163, which makes
the stencil test pass everywhere rather than fail.

**What the field round found**, on `eval/field/what-is-inside.md` — does a drop
port bottom out in a floor, or open into the gallery — asked of haiku-4.5 four
times. Every trial asked for a section unprompted, the first evidence on this
page that a new tool gets *reached for* rather than merely working. Three of four
were right; the fourth described the cut face correctly and then read it
backwards: "That black gap is solid material—it's the floor." The picture was
fine, the convention was not, and nothing in the reply said which colour meant
what. Naming it in the tool description — the flat colour is the material the
plane passed through, a dark shape inside it is void, never shadow and never
material — took the next round to 4/4.

Still unfixed, and recorded so it is not rediscovered: a section is so
convincing that it stops the model measuring. One trial in four reached a probe;
the rest answered from the picture alone. Here they were right, but a port that
stops 0.2 mm short of the gallery makes the same picture.

## 8. Diff render

The same camera, the same framing, before and after one edit, plus the numeric
deltas — volume, bounds, face and edge counts, tag set. Models are markedly
better at "what changed" than at "what is", and this is the cheapest guard
against an op that runs, returns a valid solid, and changes nothing. Small: two
evaluations, one shared `bounds` for framing (`framing_bounds` is the right
thing to share), and a subtraction. The numeric half ships without the images.

## 9. Faces as text — done, and measured on a model

What `list_entities` does for edges, done for faces — where the CAD-specific
literature has converged: [BrepLLM][brepllm], [Pointer-CAD][pointercad] and
[AgentsCAD][agentscad] all serialise the face-adjacency graph as text alongside
the render. `list_entities` now returns `faces` beside `edges`: `face@N`, kind,
exact area from `BRepGProp` rather than from triangles, centroid, outward normal
or axis, radius, and the `face@N` ids it shares an edge with. The window reads
the same numbers on hover. `Shape_faces_json` in the vendored wrapper is the
writer, deliberately separate from the full `Shape_geometry_json` a STEP
recreation needs, because the full one costs three times the time and four to
six times the bytes to serialise wires and pole grids this then discards.

Three joins are checked rather than assumed, each of which fails silently on its
own: the mesher's face runs tile the index buffer exactly; every triangle of run
*i* lies on the surface face *i* is reported to be, which is what makes "the
face under the pointer" and "the face described" the same face; and adjacency is
symmetric, so a neighbour that does not name you back fails instead of quietly
renumbering. Areas are pinned against closed forms — a bore's πdh, a drilled
face's a² − πr².

**Measured: 16/16 SOUND**, `does-the-blend-reach-the-bolts`, both models, both
arms, four trials each. Every trial reached `list_entities` and answered from
`adjacent` — the blend borders the flange top and the hub wall, no bolt hole —
against a 1.75 mm margin no render settles. Two things the grades do not show
and the transcripts do: five of sixteen cited the script's own `blend: 3` and
`boltCircle` *alongside* the adjacency, four of those Haiku, which is the soft
half of LUCKY rather than a wrong answer; and one trial read the torus's
`radius` as its minor when it is the major.

**Still absent: a face selector.** Faces can be read and cannot be named. The
window's inspector says so in as many words rather than offering a control that
does nothing, and `list_entities`' description says the route that works is the
edges around the face. That remains a change in `selectors.rs`, `selectors.ts`
and `eval/selectors.json` together — the grammar is parsed twice on purpose —
and it wants a graph op that consumes one, or it is a grammar nothing can act on.

Faces are denser per token than any image, survive a model with no vision at
all, are the natural key for §4's marks, and are the precondition for per-face
offset and for draft on an existing face (OP_ROADMAP's "whole-body offset, not
per face"). §5's field round is the strongest evidence for naming them: handed a
correct thickness between `plate` and `drilled`, half the trials described the
wrong wall. Cost: medium.

## 10. Slices — the idea, and the version worth building

**The idea as proposed** was a stack of ASCII grids, one character per
millimetre, a centimetre apart, and it should not be built in that form.
[ViTC][vitc] puts GPT-4 at 25.2% on *one* character rendered as ASCII art and
3.3% on short strings, and tokenisation destroys column alignment before
attention ever sees the grid. A structured slice would do better than 3%, but
the questions it would be for — is that hole round, is that wall 2 mm or 3 mm —
are exactly the fine-grid perception that fails, and §3 answers them exactly.
The cost seals it: a 100 × 100 mm part at 1 mm cells is 10 000 characters *per
slice*, so twenty slices is roughly 60 000 tokens for what a contact sheet plus
a report answers in three.

**What slices are genuinely best at**, and nothing else here covers: counting at
a known height, connectivity (is this level one solid or two), and the Z at
which either *changes*. So keep the axis scan and throw away the raster:

```
z=0.0    solids=1  area=1840mm²  x=[-30,30] y=[-15,15]
z=12.0   solids=1  area=842mm²   holes=2  ⌀5.0@(-20,0) ⌀5.0@(20,0)
z=13.0   solids=2  ...                      ← the boss splits here
```

Forty tokens instead of ten thousand, and the transitions are the interesting
part — so choose the heights adaptively at topology changes rather than every
centimetre. `sdf.rs` gives the occupancy test and `tags.rs`'s region pass
already does connected-component work on a pixel mask; a slice is that mask
taken on a plane instead of a view. Medium, and it should wait behind §3 and §5.

## 11. Let the agent render its own — hold, but not for long

[Whiteboard-of-Thought][wot] is the strongest result on this page: up to 92% on
tasks where chain-of-thought scored 0%, from nothing but letting the model write
plotting code, run it, and look at the output. The parcad version is a script
that returns a *plot* rather than a solid — a thickness histogram, a profile
curve, hole centres as an XY scatter. A hold, because the sandbox boundary is a
hard rule: agent scripts run in `script.rs`'s QuickJS sandbox, never in the
webview, and a plotting library is a new dependency inside it with a new set of
things it can reach. The narrow version — a `plot(points)` export that hands
data back to the *host* to rasterise with `render.rs`'s existing primitives —
keeps the boundary intact and is most of the benefit. Worth doing after §9,
which would supply the data.

## 12. Depth maps as an image — hold

[SpatialVLM][spatialvlm] and [DepthLM][depthlm] show a depth channel measurably
improving size and distance perception — both from photographs, where depth is
*estimated* and the map is genuinely new information. Here the buffer is exact
and already in hand, which makes the useful form of it a probe returning a
millimetre (§3). Shipping the picture would be re-encoding a number as pixels,
the inverse of this page's rule.

## 13. What the first whole-surface round found

Everything above was measured one tool at a time, on the tool that had just
changed. `field/run-suite.sh` puts every tool in front of a model at once.
**Ninety trials of Haiku 4.5**, ten cases in both thinking arms, three trials
each, then a five-case replication:

| | trials | SOUND | LUCKY | WRONG | VOID |
|---|---|---|---|---|---|
| round 1, ten cases | 60 | 44 | 7 | 7 | 2 |
| round 2, five of them again | 30 | 25 | 1 | 3 | 1 |

Per case, SOUND out of trials, both arms together:

| case | tool | round 1 | round 2 |
|---|---|---|---|
| how-thin-is-it | `measure_wall_thickness` | **6/6** | — |
| which-backend-measured | `evaluate_part`'s `backend` | **6/6** | **6/6** |
| what-is-inside | `section` | 5/6 | — |
| what-is-hidden | `regions` | 5/6 | **6/6** |
| how-big-can-the-fillet-be | refusals | 5/6 | **6/6** |
| does-the-port-meet | `probe_part` | 4/6 | — |
| what-does-the-fillet-touch | `inspect_treatment_target` | 4/6 | 5/6 |
| which-selector-holds | `check_selector` | 4/6 | — |
| put-it-where-i-can-open-it | the project CRUD | 3/6 | 2/6 |
| how-many-edges | `list_entities` | 2/6 | — |

**Reasoning was not the variable.** 36/45 SOUND with thinking off against 34/45
with it on, and the two worst cases were both *worse* with it: `how-many-edges`
went 2/3 to 0/3 and `which-selector-holds` 3/3 to 1/3. In both, the extra
reasoning was spent constructing a story for a wrong reading rather than
checking it. The fourth round on this page to say run both arms.

Four findings, in descending order of what they cost the model, all in replies
that were already numerically correct:

**`topological_edges` is twice the truth, and `list_entities` disagrees with it
in the open.** A plain cube reports `topological_edges: 24` and
`total_edges: 12`; the 2020 extrusion reports 246 and 122. `worker.rs` counts
with `shape.edges().count()`, and OCCT's `TopExp_Explorer` visits an edge once
per adjacent face, where the same walk that builds `list_entities` deduplicates
on a canonical key. A model handed both numbers noticed the contradiction,
reasoned that "visible edges" meant *visible from a camera* and therefore a
sample, and chose 246 — the only failure on this page where the model's
reasoning was sound and both of its inputs came from us.

**A selector's own operators are the characters a model escapes.** Two trials in
the reasoning arm sent `&lt;y` and `&gt;Z and &gt;Y and |X` to `check_selector`,
were told `invalid edge-selector term "&lt;y"`, and reported that the kernel
rejects `<y` — which it accepts. The message echoes the escape back and names no
fix, so nothing in the reply says the caller's own encoding is the problem:
"error messages name the fix" applied to a caller that is a model. `<`, `>` and
`|` are the whole grammar, and are exactly what gets HTML-escaped.

**`METRIC_FASTENERS` does not exist over MCP.** Asked for an M6 clearance hole
and told explicitly to use parcad's own number, three of six trials wrote a
literal — 6.5 twice, which is not an ISO 273 size at all. `clearance()`,
`tapDrill()` and `counterbore()` are named nowhere in the tool descriptions or
the server instructions, so the rule CLAUDE.md states for authors ("never a
literal in a part") reaches nobody on the socket.

**A part can be saved broken and reported as saved.** One trial cut its
clearance hole with `cylinder(3.3, 6).at(0, 0, -3)` through a 6 mm spacer
spanning −3…+3 — a blind hole halfway in — evaluated it, read
`volume_mm3: 1781.28` against an expected 1678.8, and saved it anyway.
`save_project` says "evaluate it first: saving a script that does not build
leaves the user a broken file", and this script *built*. The gate that exists
catches the rarer failure.

Two smaller ones worth not rediscovering. `regions` omits treatment tags
entirely rather than reporting them `visible: false`, which the server
instructions promise it does not do — `bore_lead_in` is a chamfer, `drawable()`
replaces it with an identity, and no pixel is ever attributed to it. And a
*vertex* selector that is empty is refused with "edge selector is empty", the
wrong noun, on the one path where the two grammars differ.

**What did not fail is worth recording too.** `measure_wall_thickness` went 6/6
including the caveat's direction, and the `rendered_by` regression — the server
instructions naming a field no reply contains — is 12/12 clean across both
rounds. Both are §3's and §5's rewritten tool descriptions still holding.

## 14. Reading a foreign B-rep — the tool is done, the reader is unmeasured

`probe_step_export`: hand it the absolute path of a STEP file from another CAD
system and it returns measured geometry — per-solid exact mass properties, every
face's surface down to B-spline pole grids, boundary loops as ordered polygons
where they are all straight lines. The same capability is `parcad --probe-step`
at the CLI; both sit on `service::probe_step` and run the reader inside the
expendable worker, because it is OCCT code on a file nobody vetted. The Fusion
recreation targets had stalled on "the sections live only in the Fusion
document"; they never did. Its first two runs earned its keep:
`UnTriangle-v3.step` contains a different body than the target header recorded,
and the body's "NURBS" walls are bilinear ruled patches — which turned an
"unmeasured surface-fit question" into an exact recreation
(`examples/fusion360/untriangle-v3.js`).

**What is measured, and what is not.** The output side is held by unit tests
(the wrapper-schema contract in `protocol.rs`, the refusal wording in
`service.rs`) and was exercised over a real MCP session against the retainer and
UnTriangle exports. Whether a *model* can read it is a separate fact and is
**unmeasured**: the field case exists (`eval/field/rebuild-from-the-export.md` —
export a part, treat the file as foreign, probe it, author from the probed
numbers, hold the result to them) but the round it was written for could not run
— Claude Code 2.1.223 connected to the MCP server and registered none of its
tools, for the old cases exactly as for the new one; eval/field/README.md's
fourth void mode records the diagnosis. Until a round runs, nothing on this page
claims a model reads this tool.

## 15. Reading the language itself — measured, and it moved the part

`read_docs`: the DSL reference generated from `app/src/dsl.ts` at build time,
plus `DSL_GAPS.md`, `GOTCHAS.md` and `OP_ROADMAP.md` compiled into the binary.
Every export and every public `Shape` method appears, held there by
`every_name_a_script_can_call_is_in_the_reference`, which compares the generated
document against the names the QuickJS sandbox actually hands a script — two
readings of one source, so a parser that stops understanding a declaration form
fails loudly instead of shortening the document.

**Why it exists.** An outside session connected over MCP, built three parts, and
wrote `mirror()` down as *impossible*. It is at `dsl.ts:416` with the exact
`union(half, half.mirror("x"))` idiom in its comment, and `examples/clevis.js`
is a seeded part whose header says it exists to demonstrate it. That session
also never found `revolve`, `cone`, `ngon`, `polar`, `repeat`, `countersink`,
`counterbore`, `tapDrill` or `clearance`, and said so plainly: *output quality
was a function of which example files I happened to read*. `pillow-block.js`
meanwhile cited `docs/DSL_GAPS.md` in a comment the reader had no tool to open.

**What was measured.** `eval/field/say-the-symmetry-once.md`, haiku-4-5,
thinking arm, 4 trials a round. It is the only *authoring* case here: the route
is the script, so the rubric's new `input: \.mirror\(` looks inside the tool
arguments, because no tool name and no sentence in the reply can show whether a
model reached for `mirror` or wrote both halves out with the signs changed.

| | reach `read_docs` | used `mirror` | volume right |
|---|---|---|---|
| round A | 4/4 | 4/4 | 0/4 |
| round B, after two doc fixes | 4/4 | 4/4 | 0/4 |

**The tool's own claim holds: 8/8 found `mirror`, none by reading a part.** The
verdict half does not. Round A failed in two ways, both silent and both
watertight. Two trials wrote `half = plate.union(boss).cut(hole)` where `plate`
is the *whole* plate, then `union(half, half.mirror("x"))` — and the reflected
copy put material back over the hole it had just cut, leaving each bore 5 mm
deep through its boss and nothing through the plate; `mirror`'s comment now says
the half has to be a half, and that failure did not recur. The other two placed
the cutter as `cylinder(r, 20).at(42, 0, 0)` — centred, so it reaches z = 10 on
a part that stands 11 mm tall, capping each hole with 1 mm of boss.
`cylinder`'s comment now says what centring means for a through-hole and names
`holeFor`; all four of round B did it anyway, identically. That one is not a
documentation gap this page can close by writing more, and the fix is more
likely a refusal or a warning than a sentence.

**Two things about running this suite** that cost most of a session. Field
trials must run against a **release** app: a debug binary raymarches a 512 px
view in about 70 s against a fraction of a second, so any case that asks for
`views` stalls, and under four concurrent trials the script sandbox's own 5 s
deadline starts firing on scripts that build in microseconds. And a
`pkill -f parcad-app` from a sibling checkout kills the app this round is
measuring; the trials then grade VOID with "unable to connect" in their errors,
which reads exactly like a broken tool.

---

## Suggested order

Done, and what each cost is in its own section: point and ray probes and
`tag_extents` (§3), the crossing that names its surface (§4's cheap half), wall
thickness (§5), section view (§7), faces as text (§9). What is left:

1. **The four §13 fixes, before anything below them.** Each is a line or a
   sentence, each is measured, and none is in a tool's *logic* — this page's
   oldest lesson. Count edges with `TopExp::MapShapes` in `worker.rs` so
   `topological_edges` stops being double; make `check_selector` recognise
   `&lt;`/`&gt;` in its input and say so; name `clearance()`, `tapDrill()` and
   `counterbore()` in the server instructions; and tell `save_project`'s caller
   to check the evaluation's numbers rather than only that it evaluated. Then
   re-run the suite.
2. **Numbered marks on the render** (§4, the visual half). The change with the
   best evidence behind it, and it makes the selector loop closeable.
3. **A face selector** (§9) — `selectors.rs`, `selectors.ts` and
   `eval/selectors.json` together, and it wants an op that consumes it.
4. **Diff render** (§8), numeric half first.
5. **Adaptive slice summary** (§10) and the default view set (§2), both worth
   measuring before building.
6. ASCII grids and depth images: not at all, for the reasons recorded above
   rather than the intention.

Each of these needs a case in `eval/cases/` that pins the *reported* numbers: a
perception tool that quietly starts describing an older part is worse than one
that is missing.

[cadsmith]: https://arxiv.org/html/2603.26512
[openecad]: https://arxiv.org/html/2406.09913v3
[ortho2cad]: https://arxiv.org/html/2607.08891v1
[creft]: https://arxiv.org/pdf/2506.00568
[ogvla]: https://arxiv.org/pdf/2506.01196
[som]: https://arxiv.org/pdf/2310.11441
[spatialvlm]: https://openaccess.thecvf.com/content/CVPR2024/papers/Chen_SpatialVLM_Endowing_Vision-Language_Models_with_Spatial_Reasoning_Capabilities_CVPR_2024_paper.pdf
[depthlm]: https://arxiv.org/html/2605.15876v2
[wot]: https://arxiv.org/abs/2406.14562v1
[vitc]: https://arxiv.org/html/2410.01733v1
[asciieval]: https://openreview.net/forum?id=qg7zOTPtg6
[asciibench]: https://arxiv.org/abs/2512.04125
[brepllm]: https://arxiv.org/html/2512.16413v2
[pointercad]: https://arxiv.org/pdf/2603.04337
[agentscad]: https://arxiv.org/html/2607.02448v2

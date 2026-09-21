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
| Region colouring with a legend | ✅ `tags::regions_by_face` | which tag owns which face, from the kernel's face lineage; key beside the frame, colours hashed from the name so two renders stay comparable |
| Where each tag is | ✅ `perceive::tag_extents`, `evaluate_part`'s `tag_extents` | §3 — one box and one centre per tag, the exact bounds of the faces that carry it |
| Which way a view looks | ✅ `RenderedView`'s `axes` | §2 — view names are absolute, and saying so found two of them mirrored |
| Edge listing with geometry | ✅ `list_entities` | centre, direction, length; sampled at 60, with the total — and it disagrees with `evaluate_part`'s `topological_edges`, which double-counts. §13 |
| Treatment target preview | ✅ `inspect_treatment_target` | plus tags whose edge set is *exactly* the target |
| Selector syntax check | ✅ `check_selector` | no geometry touched |
| Depth + normal per pixel | ~ `render::GeometryBuffer` | exists, and `model_point` ties a pixel to a millimetre — not exposed |
| Point and ray probe | ✅ `perceive.rs`, `probe_part` | §3 — exact distance at a point, every crossing along a ray with the face it went through, the wall thickness between them, on the B-rep with every treatment in it |
| Wall thickness / minimum feature | ✅ `perceive.rs`, `measure_wall_thickness` | §5 — the largest ball that fits in the material at every sampled surface point, on the exact surfaces, both faces it touches named; every feather and every wall between faces that do not meet found whatever the sample count |
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

## 3. Point and ray probes — **DONE**, and moved onto the exact kernel

`crates/parcad-occt/src/perceive.rs`, reached as `probe_part`. A point is
classified against the solid (`BRepClass3d_SolidClassifier`) and measured to
its nearest boundary point (`BRepExtrema_DistShapeShape`): the word `medium` —
material, void, or surface — answers "is this point inside the part", which
previously needed a render and a guess. A ray is intersected with every face
it meets (`BRepIntCurveSurface_Inter`), so each crossing is a point on an exact
surface, carries the tags of the face it went through, and says whether the
line passed *into* material or out of it. Two crossings on one ray *is* a wall
thickness, measured; [CADSmith][cadsmith]'s gap-at-the-joint failure is one
ray cast.

Until 2026-09-14 the same tool ran on a distance field, and three things it
had to say about itself are no longer true and no longer said:

- **A distance was a lower bound near a corner**, because the field could not
  over-estimate. It is the distance now, at a corner as on a face: inside a
  cube whose top edges are rounded at r = 3, the point (8, 0, 9) reads
  3 − √5 = 0.764 from the fillet's arc where the field read 1 to a corner
  that is not there.
- **Every fillet and chamfer was absent** — `drawable()` replaced each with an
  identity, and `omitted_treatments` named them. The exact solid has them, so
  the field is gone from the reply rather than always empty.
- **A grazing ray could march forever** and had to say `incomplete`. An
  intersection has no march; a tangent contact is reported as neither an
  entry nor an exit.

Two things about the exact answer are worth knowing. A ray through an edge is
reported once per face sharing it, so the walk keeps only crossings that
alternate in and out, and a hit within 0.1 µm of a face boundary is what the
intersector classifies as on it. And a part in several bodies is asked body by
body, each crossing and point naming its `body`; `solid_mm` sums over them.

**A thin reading says what kind of thin it is, because the list was unreadable
without it.** Two parts shipped as STLs on 2026-09-16 with defects this sweep
finds at once — 0.013 mm between a cable channel and a slot floor that met at
15°, and a Ø2.8 grille hole 0.319 mm into a screw boss. Nothing had run the
sweep; when it was run afterwards the first part listed five 0.319 mm readings
of a *75° lip*, which is a sharp edge doing what sharp edges do, and the second
flagged **1072** samples of which nearly all were intended 1 mm pocket floors,
with the real defect one unnamed line among them. So every sample is now
classified by the angle the two faces enclose — 180° less the turn between
their outward normals, the faces sharing an edge or being the same face wrapping
onto itself:

- **`feather`**, under 60°: material tapering to nothing, thinner than a
  threshold `t` over a band `t / tan(angle)` wide — 4.5 mm at 15°. What a cut
  that grazed another feature leaves, and never intended.
- **`wall`**, under 5° or between faces that never meet: a floor, a web, a rod
  measured across itself. Thin because a dimension made it so.
- **`edge`**, 60° and over: the reading every sharp edge gives beside itself,
  0.32 mm at 75°. Counted as `below_threshold_at_edges`, listed after
  everything else, and never the part's `thinnest`.

Samples are then grouped into places — neighbours within 2.5 sample spacings,
and a feather or an edge along one seam however far apart it was sampled — so a
place carries `samples` and `extent_mm`: one thin corner and a pocket floor thin
over 16 × 16 mm are different entries. Each face is named by geometry where no
tag names it (`cylinder r 1.40 along +z near (-34.0, 18.0, 14.8)`), which is what
identified the boss the grille had cut into. On the two shipped parts the new
report reads: feather 0.013 `cable`→`slot` first, then feather 0.319 between the
two cylinders, and `below_threshold` 3 and 12 where it had been 25 and 1072.

**Measured, not assumed, and pinned in `eval/cases/` now that there is one
kernel to pin against.** `probe-bored-block` holds the 40 mm plate with a Ø12
bore to its closed forms — crossings at x = −20, −6, 6, 20, so 14 mm of wall
and 28 in all, named plate, bore, bore, plate; the origin 6.000 from the wall
in void; a point above the bore 97.185 from the rim it is nearest, not the top
face it is above. `probe-port-meets-gallery` holds the manifold below,
`two-boxes` a ray and two points across a part in two bodies, and
`shelled-box` a ray through both walls of a shell. The same forms are unit
tests in `perceive.rs` and `service.rs`. `probe_part` has no `resolution` and
no march; what it has is a `timeout_s`, because the part is built in the
worker like an evaluation.

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

**In `eval/cases/` now.** A case there used to be a two-backend geometry
comparison, and a probe was available on one backend only; with the exact
kernel the only one, a case carries an optional `perception` block — rays,
points, a thickness minimum — held to closed forms written by hand and never
rewritten by `--update`, with the derivation in the case's `why`.

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

`perceive::tag_extents` bounds the faces of the finished part that carry each
tag — the faces the kernel's own history says the tag still owns, followed
through every boolean, blend, fillet and unify — with `BRepBndLib::AddOptimal`
on the exact geometry, so the box is the surface's own reach: `examples/
flange.js` reports its bore 23.900 deep to the micron, where a sample of the
mesh once reported it as a flat ring and then as 11.95. A tag no face carries
has no extent and says so in `unlocated_tags`. What the move settled:

- **An extent is inclusive where a colour is exclusive.** A face carries every
  tag its lineage gives it, nearest first; a pixel takes one colour and gets
  the nearest, so a union's or a cut's own tag shows no pixels of its own in
  a region map while its box covers everything it names. Nested tags report
  nested boxes, which is what the script says.
- **A tagged copy is nearer than what it copied.** "Nearest" is the tag
  closest to the node that produced the face, and a move, turn, scale or
  mirror produces its copy's faces: `left.mirror("x").tag("right")` carries
  both names on every face, and until 2026-09-15 painted the whole of
  `split-halves` `left`, `right` 0 pixels. A tag on the transform now outranks
  every tag inside its input, on the copy only; an untagged placement names
  nothing, so `cylinder(..).tag("bore").at(..)` inside a tagged body is still
  `bore`. Only `split-halves` among the 97 corpus and seed scripts writes a
  tag over a tagged input, so no other recorded surface moved.
- **A coplanar merge carries a face across two features.** The bracket's
  `plate` reaches z = 40 because its −X face merged with the wall's at the
  union and the merged face carries both names — the same face `on: "plate"`
  would select. The box is of the faces, not of the primitive.
- **A blend carries names now.** A blended union or cut used to drop its
  lineage, so every tag on `examples/bracket.js` came back unlocated; the seam
  fillet is followed like an authored one, and the blend's faces take the
  names of the faces its edge lay between. The corpus's geometry did not move.
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
was asked for alongside it and is **not** built on `tag`. `tag` is semantic: it
names what a thing *is*, and that name does real work in `probe_part` and
`measure_wall_thickness` output. Appearance came later as its own method,
`.material({ color, roughness, metalness })`, and is kept apart from every
answer: it is read off the graph per body (`Doc::body_materials`), never
followed through the kernel, so it cannot reach `tags`, a selector or the
build cache. Per body, not per feature, because a first per-feature version
drew a wall unioned into a plate half blue and half grey: the faces the union
merged belonged to both, and no rule for such a face looks like one solid.
The window wears it. An agent's render stays neutral grey unless it passes
`materials: true`, because grey is what every reading rule above was measured
on; the snapshot's `materials` count is how it knows there is anything to ask
for. The rasteriser draws the colour only. The diagnostic case — *this one
feature in magenta, the rest grey* — is still `regions`.

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
named by the faces' own tags — and a count of samples below a caller-supplied
threshold, so "one bad spot" and "the whole wall is thin" are distinguishable.
Since 2026-09-14 it runs on the exact solid, and since 2026-09-16 the number
is the one a mould or casting check means: at each sampled surface point, the
diameter of the largest ball that fits inside the material touching the
surface there (`inscribed` in `perceive.rs`). Before that it was the material
along a ray fired inward from the point.

- **How the ball is found.** Balls tangent at one point on one side are
  nested, so the radii that fit are an interval. A ball that does not fit has
  a boundary point inside it, and the ball through that point tangent at the
  sample is smaller and still no smaller than the answer — the shrinking-ball
  step (Ma, Bae, Choi & Rhee, 2012) — so a radius reached that way that fits
  *is* the answer. "Is anything closer than r" is `NearestBoundary` in the
  vendored wrapper: every face's `Extrema_ExtPS` and every edge's
  `Extrema_ExtPC` built once and visited nearest box first, where
  `BRepExtrema_DistShapeShape` rebuilt them per question. Measured: a median of
  3 to 7 radii per sample, 16 at most, over the corpus and a 180 mm lamp.
  Since the thin-detect work it also reads each face's own triangulation: a
  face whose triangles are farther than the best so far, plus a measured slack,
  is skipped whole; a B-spline face is answered by Newton from its nearest
  triangles instead of `Extrema_GenExtPS`, which rebuilt a sample grid per
  point; and inside/outside is `BRepTopAdaptor_FClass2d`, built once per face,
  instead of a classifier that intersects every pcurve per point. A ball on a
  screw thread went from 3 ms to a fraction of one.
- **Where it measures from.** Every triangle's middle, points over it no more
  than the sample spacing apart (in rows along its longest side, so a sliver
  gets one row), then the tessellation's nodes — the first of those in each
  cell `spacing / √3` across, per face, so every point of every face is within
  the spacing of a sample on it (`sample_spacing_mm` in the reply). The
  spacing is the finest, down to a hundredth of the diagonal, whose samples
  fit `max_samples`. Each is evaluated on its face at the triangle's own
  interpolated parameters, for the exact point and normal — the ball must be
  tangent to the surface, not to a chord. Until the thin-detect work the
  candidates were decimated with a stride, which left a screw thread's faces 8
  samples each against a body cylinder's 2948, and whether a small face was
  measured at all depended on the budget. A point on a convex edge fits no
  ball at all (the neighbouring face cuts every one), so it is moved 0.55 × the
  threshold into its face: beside a right-angled edge a ball there reads 1.1 ×
  the threshold and is not counted, and a wall thinner than the threshold
  still is. The probe that tells convex from concave can be fooled where a
  point lies on the line of another edge (`BRepClass` misjudges a point on an
  edge's extension), so a boundary point whose ball comes out narrower than
  four deflections moves too.
- **A seam under a quarter of a degree is smooth.** The lamp's ruled strips
  meet at creases of 0.08°, convex, and a node on one fits no ball in exact
  arithmetic; with a tolerance of 1e-7 of the radius the sweep reported 25
  samples of "edge" at 0.1–0.18 mm down the middle of a 1.2 mm wall. Boundary may reach
  1e-5 of a ball's radius into it and the ball still fits (`BALL_SLACK`), and
  the edge probe uses the same share, so the two agree on what a crease is.
- **The kind comes from the ball.** `wedge_deg` is 180° less the angle between
  the ball's two contacts seen from its centre — 0 across a wall, 90 in a box's
  corner — so it no longer needs the faces to share an edge. A ball wedged at
  60° or more is an `edge` reading whether or not a round sits between the two
  faces: every round reads its own diameter, 2r, and is an edge.
- **Cost, and its complexity.** Per sample, one projection and k ≈ 3–16
  nearest-point tests; each test scans every face's box (O(F), a few µs at
  F = 5000) and projects onto only the faces and edges whose boxes lie within
  the current radius, which for a thin wall is a handful. So
  O(S · k · (F + m)) for S samples and m faces near the ball, plus O(F + E)
  projector set-up per body.

| part | faces | ray sweep: thinnest, below 1.2 / at edges, time | ball: thinnest, below 1.2 / at edges, time |
|---|---|---|---|
| lamp shade, 1.6 step on sloped walls, open ends | 82 | wall 1.211, 0 / 4, 123 s | wall 1.211, 0 / 68, 52 s |
| `thickness-under-a-fillet`, 8 plate, r 2 rounds | 10 | wall **7.079**, 0.5 s | wall **8.000**, 0.5 s |
| `shelled-box` | 12 | wall 2.000, 0.3 s | wall 2.000, 0.1 s |
| `fitted-ring` (B-spline) | 4 | wall 2.000, 10.6 s | wall 2.000, 6.8 s |
| `probe-port-meets-gallery` | 10 | wall 5.000, 0.5 s | wall 5.000, 0.5 s |
| `twisted-planter` | 245 | wall 2.000, 0 / 61, 5.3 s | wall 2.000, 0 / 383, 7.0 s |
| `flange` | — | wall **4.814** | wall **6.300** |
| `knurled-knob` | — | wall 0.916, 941 / 81 | wall 0.983, 529 / 1751 |
| `timing-pulley` | — | feather 0.006 | feather 0.005 |

Times are whole calls, part build included (the lamp builds in 4 s), on a
machine shared with five other builds. What the rows say:

- **The lamp's rims were already classified** — `f32be230` made them `edge`
  readings before this — so the ball changes no verdict there. It agrees with
  the ray within 1 % on 98.8 % of wall samples and never reads above it (0 of
  4895; a missed nearest point would show as exactly that), and it is cheaper
  on long B-spline strips: 16 s of balls against 91 s of rays in the same
  sweep. Its thinnest wall is 1.211 and not 1.6 because the script steps its
  sections 1.6 mm in the *plane*, so on a sloped wall the wall is
  1.6 · cos(slope). Both methods say so.
- **Where the two disagree, the ray was wrong.** A rounded 8 mm plate read
  7.08 from lines leaving the underside through the round at a slant; the
  shell read walls of 64 mm from a line 0.04 mm below the cavity's floor, the
  lamp one of 78 mm. The ball has no line to leave by.
- **The flange changes answer, and that is the definition.** Its back-face
  countersinks bring a bolt hole's rim to 4.80 from the OD on that face; the
  ray read 4.81 there from lines leaving through the chamfer. No ball wider
  than the corner fits there, just as beside any sharp edge, so the thinnest
  wall is the 6.3 ligament. `eval/field/how-thin-is-it.md` now asks for 6.3.
- **Edge readings are many more.** A ball reads thin beside *every* sharp edge
  and round, on both faces; a ray only where it happened to leave through the
  neighbour. `below_threshold_at_edges` is a count of that, not of defects.
- **Finding a small defect was sampling**, until the searches below. Over ten
  sample budgets from 4000 to 24000, on the two field-instrument parts as saved
  in the user's folder: the 0.013 mm cable-to-slot sliver, ray 10/10 and ball
  9/10; a 0.457 mm sliver between a screw hole and a foot recess, ray 7/10 and
  ball 10/10; a grille hole cut 0.319 mm into a screw boss, ray 3/10 and ball
  2/10 (the ball read it at 0.155, nearer the zero it tapers to). Before the
  edge step the ball found the first 7/10 and the last 0/10.

Closed forms in `eval/cases/`: `slanted-slab` (2 between the faces, 2.3094
straight down through them — the two definitions a cosine apart),
`eccentric-tube` (1.000 where the circles are nearest), `conical-shade` (the
lamp in closed form, 1.5522 with the rims as edges), and
`thickness-under-a-fillet` now holds 8.000.

- **The minimum was a sampled minimum, exact at its own point.** The
  manifold's outboard wall read 5 + y²/15 from a sample y off the port's
  generator, where a ball tangent to the plane meets the Ø10 port. It now reads
  5.000: the face-pair search below settles on the generator itself.

### What is certain — the edge and face-pair searches

A thickness tool that misses a real sliver is wrong, and raising the sample
count only makes a miss less likely. So the sweep is now the third of three
searches, and the first two do not depend on it (`thickness` in
`perceive.rs`, with `edge_wedges` and `close_pairs` in the vendored wrapper):

1. **Every edge is read along its length.** At most a four-hundredth of the
   diagonal apart and at least eight times, the angle the material encloses
   between the edge's two faces — from each face's normal and the direction
   into it, which is the normal crossed with the edge as oriented in that face,
   checked once per edge by stepping along it and classifying (the pocketed-box
   test: 24 edges, 16 at 90°, 8 at 270°, no correction needed). An edge that
   encloses less than 60° anywhere is a **feather**, and it is reported at
   **0 mm**, on the edge, at its sharpest, with that angle and the stretch
   that sharp as its extent. That is how thin a feather gets: the sampled
   0.013 on the desk stand, and 0.007 to 0.304 over budgets, were only how
   near the seam a sample happened to land.
2. **Every pair of faces that share no edge and come within reach is solved on
   the surfaces.** The reach is the threshold, or the thinnest wall the sweep
   sampled if that is thicker (so an exact `thinnest` comes with no
   threshold too). Candidate pairs come from the two faces' triangle trees:
   the surfaces lie within a measured slack of their triangles, so a pair whose
   surfaces come nearer than the reach has triangles within reach plus slack,
   and no such pair is missed. From the nearest triangles, damped Newton on
   both surfaces settles the double normal — the two points whose segment is
   square to both faces — kept inside each face's parameter box. When it is
   inside both faces and each face's material is towards the other, the ball
   is taken there; a least distance along a line (a hole beside a flat side,
   two parallel cylinders) may settle at an end of the line where no ball fits,
   or where a third face cuts it, so it is settled again from where the sweep
   found this wall and from points stepped along the face, and the first ball
   that spans the whole distance is the reading. It is the closed form: 0.4569
   for the control box's plate, 7.000 for the bracket's hole-to-edge ligament
   the sweep read as 7.009, 5.000 for the manifold.
3. **The sweep**, as above, then the four thinnest sampled walls followed
   downhill on their faces by a compass search down to a micron.

So the guarantee, which the reply's `note` states: **every feather is found,
however short, and every wall thinner than the threshold between two faces
that do not meet is found and read where it is thinnest.** What rests on the
samples is a wall of another shape: across a single curved face (a thin pin,
a tube drawn as one surface), between faces that meet elsewhere, or within
twice the reach of a vertex two faces share; those are found where they are
wider than `sample_spacing_mm`.

Two readings are not minima, and say so in the docs rather than the number:

- **A least distance on a face's boundary** — a countersink cone coming near a
  side face at its rim — is near a wall, not across one. There the first wall
  reading stepped in from it is followed back towards the boundary by halving
  while it stays a wall, and the thinnest kept; that reading is real, and the
  thinnest near there is within about a percent of it. It is searched for
  below the threshold, and without one below the thinnest sampled wall, which
  is why the pipe tee reads 3.913 with no threshold and 4.050 at 1.2.
- **A ball smaller than twice the mesh deflection against a face its own face
  meets** is an `edge` reading: the point is on the edge, and a feather there
  is reported by the edge search. A ball that small against a face that does
  not meet is a wall — `pierced-membrane` holds a 0.004 lid as one.

**Measured, 2026-09-16.** The three defects that were found only some of the
time, on copies of the field-instrument parts as saved in the user's folder,
through `parcad call measure_wall_thickness` at threshold 1.2, over ten
budgets from 4000 to 24000:

| defect | ball sweep alone | with the searches |
|---|---|---|
| desk stand: cable channel's ceiling meets the 15° slot floor | 9/10, read 0.007 to 0.304 | **10/10**, feather 0 at 15.000°, on y = −13.768 (−17.5 + 1/tan 15°) |
| control box: M2 hole beside a Ø8 foot recess, four corners | 8/10, all four 1/10, read 0.464 to 0.707 | **10/10**, all four, 0.4569 (5.6569 − 4 − 1.2) |
| control box: grille hole through a screw boss | 4/10, read 0.144 to 0.41 | **10/10**, feather 0 at 51.75° |

And ten more with each part turned about Z and moved by seeded random amounts
up to 360° and 3 mm, at a random budget each — a different mesh, sample grid
and face order every time: 10/10, 10/10 with all four corners, 10/10. The
readings did not move. `eval/cases/` holds each shape in closed form at 200
samples: `hairline-sliver` (0.2500 between two cylinders; the sweep alone read
1.000 at 200 and 0.265 at 6000), `ramp-feather` (0 at 15.000°),
`pinched-boss` (0 at 51.753°), `pierced-membrane` (a 0.004 lid).

**Cost.** Interleaved medians of three, HEAD's sweep against this one, the
same test-profile build, threshold 1.2 unless it says otherwise (the call adds
the part's build, which neither changes):

| part | before | after | thinnest before → after |
|---|---|---|---|
| lamp shade (82 B-spline faces), no threshold | 26 912 ms | 3 682 | wall 1.2108 → 1.2007 |
| `fitted-ring` (B-spline) | 6 100 | 247 | 2.000 → 2.000 |
| `spur-gears` | 9 337 | 2 256 | wall **0.0002** → 2.1132 |
| `twisted-planter` (245 faces) | 4 536 | 1 510 | 2.000 → 2.000 |
| `plate-stand` (86 k mesh nodes) | 5 156 | 1 456 | 6.000 → 6.000 |
| `pipe-tee` | 1 956 | 277 | 4.050 → 4.050 (3.606 with no threshold) |
| `cast-foot` | 1 241 | 102 | 4.7578 → 4.7292 |
| `wash-bottle` (median of 7) | 1 426 | 1 156 | 1.200 → 1.200 |
| `screw-top-jar` | 802 | 713 | feather 0 → 0 |
| control box | 500 | 334 | wall 0.4686 → feather 0 |
| desk stand | 200 | 73 | feather 0.0343 → 0 |
| `hydraulic-line` | 188 | 179 | 1.500 → 1.500 |
| `conical-shade` | 82 | 71 | 1.5522 → 1.5522 |
| `bracket` | 243 | 96 | 7.0088 → **7.000** |
| `cover-plate` | 195 | 86 | 7.2924 → 6.7882 |
| `diamond-v19` | 198 | 65 | 35.5507 → 35.5842 |

Every example and thickness case, in both arms, is at least as fast: from
1.02× (the hydraulic line, within noise) to 25× (`fitted-ring`); the lamp
without a threshold is 7×. The wash bottle's first round of three read 1.5 s
against 1.9 without a threshold, and seven rounds read 1.43 against 1.17 —
most of its time is the mesher's, which neither version changes. What moved,
and why:

- **Faster because a B-spline ball got cheap.** `Extrema_GenExtPS` rebuilt a
  grid per point and `BRepClass_FaceClassifier` intersected every pcurve per
  point; the triangulation and `BRepTopAdaptor_FClass2d` replace both, and
  `BRepExtrema_DistShapeShape` — 40 to 60 ms a face pair between B-spline
  edges — is not used at all. The old decimation also hid a cost: the jar's
  thread faces had 8 samples each, and a fair share of samples on them is
  what first made the new sweep ten times slower before the query was fixed.
- **Exact where the sweep was not.** The bracket's ligament is 7.000 (a Ø6
  hole 7 from the edge); the manifold's is 5.000; `pillow-block` 4.750.
- **Thinner where the searches found what samples missed.** The spur gears'
  0.0002 "wall" was a sample on an edge; they read 2.113 now. `cover-plate`
  reads 6.788 where a countersink comes near the side face (the ligament
  below it is 7.25), and the lamp 1.2007 across a sloped strip.
- **Slightly thicker, once:** the diamond at threshold 1.2 reads 35.584
  against the old sweep's 35.551 — a wall between facets that meet, which
  rests on the samples; the fairer spread landed elsewhere, and the downhill
  polish found that basin's minimum rather than the other's. Without a
  threshold it reads 35.488.
- **Feathers are 0.** The enclosure, the jar and the timing pulley already
  had feathers; they read 0 now instead of a sample's distance from the seam.

**Measured on a model**, 2026-09-16: `eval/field/where-is-the-sliver.md`, Haiku
4.5, four trials per arm, a ten-line part with a 15° feather and a real
0.725 mm wall beside it. The first round was **0/4 SOUND**: every trial called
the tool, read the feather at 0, and answered with the wall — "a grazing
intersection artifact rather than intentional wall material". The reply then
described a feather as two faces meeting at a shallow angle, which reads as an
edge, and the prompt asked for thin material "not merely beside a sharp edge".
With the tool description, the `kind` schema and the `note` saying a feather
is a sliver of real material that thins to a knife edge, never an artefact,
the same prompt scored **2/4**; with the prompt asking plainly for the
thinnest material, **8/8** over both arms. A 0 reads as nothing unless the
reply says what it is.


**Measured on a model, after the ball**, 2026-09-16: Haiku 4.5, four trials
per arm. `how-thick-is-the-shade` (new): 8/8 SOUND, every trial 1.552 mm, and
the transcripts name the rim readings as edges "not actual wall thickness" —
the `kind` was read. `how-thin-is-it`, its answer moved from 4.8 to 6.3 by the
new definition: 8/8 SOUND. Two thinking-off trials trip the `19.1` trap only by
quoting the hole's diameter while naming the surface.

**Measured on a model**, before the ball: `eval/field/how-thin-is-it.md`, four trials of Haiku
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

### Bed fit — done, the flat half of printability

Every report now carries `prints_on`: the part's size against the Bambu
beds (A1 mini, the 256 mm A1/P1/X1, the H2D), lying flat as drawn or turned
a quarter turn, with the axis and the millimetres by which the others miss.
The CLI prints it as one line after `stands`. The V holder was designed
around a 256 mm bed and its report never said whether it fit one until the
user asked. It is conservative on purpose: a diagonal placement that would
fit is reported as not fitting, because a slicer's auto-orient is the
tool for that and this is the tool for "split it or not". Overhangs and
supports are still §6's open question.

## 7. Section view — done

Filed on OP_ROADMAP as §8, a view concern rather than an op, and the single
thing most missed while writing the corpus (DSL_GAPS §0). For an agent a section
is the only way to see an internal feature at all, and `intersect(part, box(...))`
is the wrong answer because it produces a *different part*. `evaluate_part` takes
a `section` and nothing about the part changes: it is how the picture is drawn.

**One `Section`, two renderers.** `view::Section` is an axis, a position and a
side, and both defaults resolve *per view*, because the half that has to go
depends on where you are looking from. The window clips on the GPU; the
rasteriser, which is what an agent's renders come off, has to cap the hole
the clip leaves or a solid boss draws as a thin cup.

**Capping is a parity count, and the textbook answer was wrong here.** Each
pixel counts the crossings the clip threw away; odd means the ray was still in
material at the plane, so that pixel is cut face. The signed-winding version —
+1 for a face turned toward the viewer, −1 for one turned away — is the standard
and needs each crossing's *facing*, which means the mesh's normals, which dual
contouring does not have at a sharp feature: on a plain cube it reports the top
face's normal along a vertical edge, and a third of the part comes back falsely
capped. Parity needs no normals. Both renderers now agree pixel for pixel, which
is the test that caught it.

**Parity is per body, and every crossing is counted once.** Two named bodies
drawn through each other are two closed shells, and one parity over both reads
the material they share as empty: `interfering-bodies` drew the boss's buried
5 mm as a hole in the plate (2026-09-15). The raster keeps one parity bit per
body, from the triangle runs the worker already records per body, and a pixel
is cut face if it is inside any body — the per-object capping three.js's
stencil examples do by clearing the stencil between objects, and what the
window's signed stencil count already gave it (two overlapping shells count
±2, not 0). And a parity is only as good as its count: a sample on a shared
edge used to fall to neither triangle in f32, which drew a line down an M10
bolt's cut face; GOTCHAS, "A section cap with a line through it".

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
centimetre. The kernel's point classifier (`perceive.rs`) gives the occupancy
test; a slice is a mask of it taken on a plane, and counting its components is
what the region pass already does on a view. Medium, and it should wait behind
§3 and §5.

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

Two smaller ones worth not rediscovering. `regions` used to omit treatment
tags entirely rather than reporting them `visible: false` — `bore_lead_in` is
a chamfer, `drawable()` replaced it with an identity, and no pixel was ever
attributed to it; since the region map colours by the kernel's face lineage
every tag is listed, and a chamfer's faces carry the names of the faces its
edge lay between. And a *vertex* selector that is empty is refused with "edge
selector is empty", the wrong noun, on the one path where the two grammars
differ.

**What did not fail is worth recording too.** `measure_wall_thickness` went 6/6
including the direction of the caveat it carried then, and the `rendered_by` regression — the server
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
trials must run against a **release** app: a debug binary was measured at
about 70 s per 512 px view when renders were raymarched, against a fraction of
a second in release, so any case that asks for `views` stalls, and under four
concurrent trials the script sandbox's old 5 s deadline started firing on
scripts that build in microseconds (its budget is now counted work, which a
slow binary or a busy machine does not change). And a
`pkill -f parcad-app` from a sibling checkout kills the app this round is
measuring; the trials then grade VOID with "unable to connect" in their errors,
which reads exactly like a broken tool.

---

## 16. Does it fit — done, and read by a small model

The question a holder exists to answer, and the one every tool above
sidesteps: lay the object in the part and say whether it fits. `check_fit`
(MCP) and `parcad --fit` (CLI) take two scripts, build both on the exact
kernel, and report `clear`, `touching` or `interfering`, the volume the two
share, and when they share none the clearance and the two points it is
measured between (`BRepExtrema_DistShapeShape`, exact). The reference is
usually one line from the `DEVICES` table: `return device("macbook-pro-16")
.at(0, 0, 18.4)`. The V holder at rest touches; lifted half a millimetre it is
clear by 0.500; sunk one it shares 27 653 mm³, which is the floor area the
report already gave, times one — the cross-check that says the number is
real.

Measured on a model the day it landed: `eval/field/does-the-laptop-fit.md`
asks for the clearance of a laptop placed 0.5 mm above a tray's floor, a
number the script does not contain. Haiku, four trials, thinking on: 4/4
SOUND, 4/4 reached `check_fit`, 4/4 quoted 0.5. It took three rounds to
get there and the tool was right in all three; the case was wrong twice —
a tray whose cutter missed, which every trial repaired from the refusal,
and a height given in prose, which three of four re-derived. The rubric
now carries the number and the reference script is handed over verbatim.
Two more cases landed with the selector work: `which-edges-are-the-seam`
(4/4 right, 2 SOUND, every trial reaching for `between`) and
`which-upright-edges-round` (3/4 SOUND, the fourth right without
evaluating).

## 17. Showing the user — the model sees, the user may not

Every section above is about what the *model* can see. A separate fact: in
Codex the model receives `evaluate_part`'s PNGs (its session log carries them
as image inputs, and it named the background colour right) while the user
sees none of them, so a Codex session asked to show a part fell back to
computer use. The Codex app does render a Markdown image with a local path in
the model's reply, so each kept view now also carries `markdown`, that line
ready to paste, and the server instructions say to paste it. A tag-region map
gets none: it is for the model to read.

Measured on 2026-09-14, "make a plate … show me what it looks like",
graded by `eval/field/show-me-the-part.md` (an image line naming a plain or
section render):

| client | without the field (0.0.5) | with it |
|---|---|---|
| Codex 0.147, gpt-5.6 | 1/2, and that one a region map | 5/5 |
| Claude Code, Haiku, both thinking arms | 0/8 | 4/8 |

The Haiku failures are the failure itself: "the top view shows the four holes
clearly" about a picture the user never had. Several trials instead put the
part on the live screen with `set_script`, which *is* a way to show it where
the app is open, and the case grades that WRONG — read those transcripts
before reading the score. Renders live in the temp directory, so a chat's
images outlast the machine's temp cleaning only by days.

## 18. Surfaces — what a part with no inside says

A surface part (docs/ARCHITECTURE.md, "Surfaces") changes what every
perception tool can truthfully answer, and each says so rather than
pretending: `evaluate_part` carries `kind` and `surface` — area, `open`,
free edges and their length, loops — and no volume, watertightness, bed or
print fit; `measure_wall_thickness` refuses a surface naming `.thicken(t)`,
and skips (and lists) surfaces in a mixed part; `probe_part` never calls a
point `material` near a surface, measures its distance to it, and lists each
place a ray passes through it as a crossing into `void`; a thickened part's
wall is `thickened_mm`, measured at a grid on every face before the reply is
built. Two field cases were written with the mode —
`eval/field/is-the-sheet-closed.md` (reading `surface` instead of reaching
for a volume) and `eval/field/make-the-sheet-printable.md` (whether a refusal
naming `thicken` is acted on); section 18's rounds are recorded below them
when run.

## 19. Editing in place — what a session spends on script bytes

Every section above is about what a model reads. This one is about what it
*sends*: the coin-holder session (docs/COIN_HOLDER_REVIEW.md, B1) sent 241 KB
of script in 45 calls, nineteen of them to change under ten lines, and with
one late edit to make it ran a Python old/new replacement on part.js from a
shell. Since 2026-09-20 every script-taking tool takes `project` (or
`"@session"`, the screen) and `edits` instead of a script, `edit_part` is that
replacement as a tool — built before written, snapshotted — and the server
instruction says to send a script once. `field/score.py` now reports `sent`,
the bytes of `script` a trial sent, which is the number this exists to move.

Measured 2026-09-20 on haiku, 4 trials per arm, one trial at a time on a
fresh copy of the part (`examples/fusion360/retainer-v1.js`, 146 lines,
6,561 bytes) — the two cases in `eval/field/` written with the change, and
the two screen cases as regressions:

| case | arm | before: sound / reach / bytes per trial | after: sound / reach / bytes per trial |
|---|---|---|---|
| change-one-dimension (`edit_part`) | thinking | 0/4 · 0/4 · 19,682 | 1/4 · 1/4 · 8,201 |
| change-one-dimension (`edit_part`) | no thinking | 0/4 · 0/4 · 17,496 (1 VOID) | 1/4 (1 VOID reached it) · 2/4 · 1,640 |
| what-if-it-were-thicker (`{ project, edits }`) | thinking | 0/4 · 0/4 · 8,216 | 4/4 · 4/4 · 0 |
| what-if-it-were-thicker (`{ project, edits }`) | no thinking | 0/4 · 0/4 · 9,866 | 2/4 · 2/4 · 3,280 |
| change-the-open-part (regression) | thinking | 3/3 · 3/3 · 2,408 | 2/3 · 2/3 · 1,204 (1 WRONG: never reached the server) |
| change-the-open-part (regression) | no thinking | 3/3 · 3/3 · 2,408 | 2/3 · 2/3 · 1,204 (1 LUCKY: edit_part on the project, which saved) |
| did-the-window-draw-it (regression) | thinking | 3/3 · 3/3 · 162 | 3/3 · 3/3 · 162 |
| did-the-window-draw-it (regression) | no thinking | 3/3 · 3/3 · 162 | 3/3 · 3/3 · 162 |

Before the change every trial was LUCKY by construction — the right volume,
reached by resending the whole part once to evaluate and once to save, and
the number to read is the bytes. After it, the what-if is answered the cheap
way every time with thinking on, and the model that still resends is the one
that must be read about in docs/WRITING_FOR_MODELS.md: a small model with
`edit_part` on the surface reached for `save_project` with the whole script
in 3 of 4 thinking trials, one of them after it had already measured the
change with `{ project, edits }`. The screen cases were at their ceiling
before and are worth one sentence after: asked to lengthen the open part as a
proposal, not a save, one non-thinking trial called `edit_part` with the
project's name rather than `"@session"`, which writes part.js (and puts it
on screen, since it was open), and reported the screen honestly; the
thinking-arm WRONG never reached the server. A model that has just opened a
part by name reaches for that name; `"@session"` is the spelling the case
wants and the description gives, and one trial in six did not take it.

**The checks round, 2026-09-20.** Three things from the coin-holder review
landed together (docs/COIN_HOLDER_REVIEW.md, B2, workflow §2, mechanical
§2.6): `checks: [...]` in the returned object, judged on every build and
first in every reply; the door — `export_part`, `save_project` and a saving
`edit_part` refuse a failing check unless `allow_failing` gives a reason;
`.reference()` bodies measured and drawn but in no file; and `note()`, carried
as "from the script, not measured". Measured on the same small model, 4
trials per arm (3 for the screen case), two hosts on scratch ports, the
in-place cases one trial at a time on a fresh copy:

| case | arm | before: sound / reach / bytes per trial | after: sound / reach / bytes per trial |
|---|---|---|---|
| is-it-safe-to-save (the door, `allow_failing`) | thinking | 0/4 · 0/4 · 994 (4 LUCKY) | 4/4 · 4/4 · 0 |
| is-it-safe-to-save | no thinking | 0/4 · 0/4 · 3,959 (4 LUCKY) | 4/4 · 4/4 · 248 |
| is-the-catch-still-caught (`checks` of `{ project, edits }`) | thinking | 1/4 · 1/4 · 875 | 2/4 · 2/4 · 450 |
| is-the-catch-still-caught | no thinking | 0/4 · 0/4 · 1,009 (2 VOID) | 0/4 · 1/4 · 675 (3 LUCKY, 1 VOID) |
| where-is-the-sliver (§2.6's risk) | thinking | 4/4 · 4/4 · 402 | 4/4 · 4/4 · 287 |
| where-is-the-sliver | no thinking | 2/4 · 4/4 · 287 | 1/4 · 4/4 · 287 |
| change-one-dimension (regression) | thinking | 2/4 · 2/4 · 4,918 | 3/4 · 3/4 · 3,278 |
| change-one-dimension | no thinking | 1/4 · 2/4 · 4,920 | 3/4 · 3/4 · 1,640 |
| what-if-it-were-thicker (regression) | thinking | 4/4 · 4/4 · 0 | 3/4 · 3/4 · 1,639 |
| what-if-it-were-thicker | no thinking | 1/4 · 1/4 · 3,292 | 0/4 · 0/4 · 8,200 |
| change-the-open-part (regression, screen) | thinking | 3/3 · 3/3 · 2,006 | 2/3 · 2/3 · 2,408 |
| change-the-open-part | no thinking | 3/3 · 3/3 · 2,408 | 2/3 · 2/3 · 1,605 |

The door case is the one this round exists for, and it is the whole story
in two rows. Before: eight trials of eight reported the part saved and named
the failing check with its 0.13 mm, and every one is LUCKY — the before host
has no `checks` key, so the part cannot build there, `save_project` wrote it
anyway (built: false, as it always did), and the 0.13 came from the script's
own header comment; two thinking trials deleted the `checks` entry to make the
part build and put it back afterwards. After: eight of eight went through the
door — refused once, then `edit_part` with `allow_failing` and the user's
reason, 0 bytes of script sent — and quoted the check as the refusal named
it. The trap (a save reported as done with the check unmentioned) fired in
none of sixteen trials on either host, because both hosts' replies name the
check: one from a comment, one from a measurement. The catch case reads the
same way with a smaller signal: after the change every trial that reached
a reply said `failed` with 16 mm³, and the LUCKY ones are the resend — the
whole 900-byte part sent as `script` instead of `{ project, edits }`, the
behaviour §19's first table already recorded. `note()` was never called in
any transcript of either round, so the sliver case measures the risk §2.6
named only as an absence: no trial quoted a note, and the non-thinking arm's
0.725 mm wall (NEXT.md, "Left from the engine wave", item 10) is the same
wrong answer as before.

Two regressions to read rather than count. The screen case's two LUCKY trials
each changed the screen through `edit_part "@session"` — the route the server
instruction has named since the edit-in-place batch — and the rubric, written
before that tool existed, requires `set_script`; the revision and the viewer's
25 mm in their verdicts came from tool replies, and the case is the thing to
update. The what-if's non-thinking arm resent the whole part in all four
trials against one of four before (8,200 bytes against 3,292), with the
volume right every time; the thinking arm's one LUCKY did the same. Nothing
in this round touched that tool's route; the evaluate_part description grew a
paragraph on `checks` and reference bodies, and whether a longer description
moves a deferred-tool client's first call is the unmeasured variable —
docs/WRITING_FOR_MODELS.md, "What parcad measured".

**The print-check round, 2026-09-21.** docs/NEXT.md's item 1 landed whole
in four commits: `collisions` in every reply (what each cut took from the
features it was not for), `print_check` beside the author's `checks` on every
build with the door reading both slots through one `allow_failing`, overhang
per body in the orientation it prints (`.printedUp()`), and `print/<body>.3mf`
written on save with a Print button in the window. Measured on the same
small model, 4 trials per arm (3 for the screen case), two hosts on scratch
ports — the before host at d71392a8, the after host at the batch's last
commit — the in-place cases one trial at a time on a fresh copy, the screen
cases with a browser tab fronted on each host and the screen reset between
trials. `field/run-case.sh` now passes `--strict-mcp-config`: a `parcad-web`
relay entry scoped to the home directory was loaded beside the server under
test, and 14 of 32 trials of the first before round answered nothing but that
it had failed to connect; that round was discarded.

| case | arm | before: sound / reach / bytes per trial | after: sound / reach / bytes per trial |
|---|---|---|---|
| is-this-ready-to-print (`print_check`, new) | thinking | 1/4 · 1/4 · 0 (1 LUCKY, 2 WRONG) | 2/4 · 2/4 · 0 (2 LUCKY) |
| is-this-ready-to-print | no thinking | 0/4 · 1/4 · 0 (4 LUCKY) | 2/4 · 2/4 · 0 (1 LUCKY, 1 WRONG) |
| where-does-it-need-support (overhang, new) | thinking | 0/4 · 1/4 · 0 (4 WRONG) | 4/4 · 4/4 · 0 |
| where-does-it-need-support | no thinking | 0/4 · 0/4 · 0 (4 WRONG) | 1/4 · 2/4 · 0 (1 LUCKY, 2 WRONG) |
| is-it-safe-to-save (regression) | thinking | 4/4 · 4/4 · 746 | 4/4 · 4/4 · 1,245 |
| is-it-safe-to-save | no thinking | 4/4 · 4/4 · 993 | 4/4 · 4/4 · 1,242 |
| is-the-catch-still-caught (regression) | thinking | 2/4 · 2/4 · 450 | 1/4 · 1/4 · 675 |
| is-the-catch-still-caught | no thinking | 1/4 · 1/4 · 675 | 1/4 · 1/4 · 675 |
| change-one-dimension (regression) | thinking | 1/4 · 1/4 · 8,199 | 4/4 · 4/4 · 0 |
| change-one-dimension | no thinking | 2/4 · 2/4 · 6,561 | 1/4 · 1/4 · 6,561 |
| change-the-open-part (regression, screen; rubric fixed) | thinking | 3/3 · 3/3 · 2,424 | 3/3 · 3/3 · 2,407 |
| change-the-open-part | no thinking | 0/3 · 0/3 · 0 (2 LUCKY, 1 WRONG) | 2/3 · 2/3 · 802 (1 LUCKY: edit_part "@session", no evaluate) |

The two new cases are the round. On the before host the ready-to-print part
builds and the model has the sweep, so the feather is found by every trial
that runs `measure_wall_thickness`; the collision is not in any reply there,
and the answers that name `grille cuts boss` got it from probing the boss's
side with rays (LUCKY: `evaluate_part` never read) or guessed the wrong pair
(`channel cuts floor`, WRONG). After: every trial that called
`evaluate_part` read the verdict off `print_check` and said both — READY no,
the 0 mm feather between `floor` and `channel`, `grille` cuts `boss` — and no
trial in either arm read the failing verdict as ready (trap 0 of 16). The
trials that did not reach it took the same route as before: open the part,
sweep, probe — `evaluate_part` never loaded — which is the finding in
docs/WRITING_FOR_MODELS.md. The support case is the cleanest number here:
0/8 before (no reply carried an overhang, and every trial computed one from
the source or from rays and got a different figure) to 4/4 with thinking on,
all four quoting 65.797 mm² off `print_check.bodies[0]` and naming `lip`;
without thinking, the trials that never called `evaluate_part` probed the
lips with rays and summed their own areas.

The regressions hold where they were measured before, within the noise of
four trials: the door case is 16/16 on both hosts; the catch case's LUCKY
trials are the whole 900-byte part resent as `script`, exactly as in the
checks round; change-one-dimension's thinking arm went from 1/4 to 4/4 with 0
bytes sent and its plain arm from 2/4 to 1/4 with the same bytes, which four
trials cannot separate from noise. The screen case's rubric no longer
requires `set_script` — `edit_part "@session"` is the route the instructions
name — and its one LUCKY after is a trial that changed the screen through
`edit_part` and never called `evaluate_part`, measuring the 25 mm off the
edit's own reply. The before host's plain arm reads the same way: one trial
edited the part by name (which saved it, and put it on screen because it was
open), one edited `"@session"` without evaluating first, and one changed
nothing and asked whether it should.

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

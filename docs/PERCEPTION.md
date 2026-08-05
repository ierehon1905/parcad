# What an agent can see, and what it should be told instead

parcad's MCP surface exists so a model that cannot open the window can still
work on a part. This page is the counterpart of [OP_ROADMAP.md](OP_ROADMAP.md):
that one asks which *operations* to add, this one asks which *perceptions* — the
tools that let an agent notice, inspect and measure a solid it has just built.

The yardstick here is not Fusion. It is the measured behaviour of the models
themselves, which is well enough documented by now to design against rather than
guess at. The deciding rule falls straight out of it:

> **Anything an agent could answer by squinting at a picture gets answered with
> a measured number instead.** A render is for shape, presence and "is this the
> part I asked for". A number is for dimension, count, thickness and clearance.
> The two ship in the same reply, and where they disagree the number is right.

That is the project's existing "report measured values, not requested ones" rule
pointed at the *reader* rather than the author, and it is why several entries
below are marked *hold* — not because they are hard, but because a cheaper tool
already answers the question the expensive one was for.

---

## What the models actually do

Worth stating once, with sources, because most of the design follows from it.

**They are good at** overall shape, presence or absence of a requested feature,
gross topology, obvious symmetry breaks, and "does this look like a bracket".

**They are bad at** metric dimensions read off pixels, small gaps and failed
booleans, thin-wall violations, counting past about six, and mental rotation
between two views. The most useful single data point is
[CADSmith][cadsmith]'s: a quadcopter frame scored IoU 0.985 and passed every
vision check by a Claude Opus judge, while containing gaps between the arms and
the hub that "three fixed views cannot resolve". The failure that reaches a
machine shop is exactly the failure a render hides.

Findings that changed decisions on this page:

- **Three orthographic views plus an isometric is the ceiling, not a floor.**
  [OpenECAD][openecad] measured *four* views performing **worse** than fewer —
  the vision tower drowns and the extra views add irrelevant detail. Our
  seven-panel contact sheet is a default worth revisiting per §2.
- **Orthographic beats perspective for anything dimensional.**
  [OG-VLA][ogvla] renders orthographic specifically to remove the ambiguity;
  [Ortho2CAD][ortho2cad] and [CReFT-CAD][creft] work from front/top/right with
  dashed hidden edges and burnt-in dimensions.
- **Overlaid identifiers unlock grounding.** [Set-of-Mark][som] is the single
  most reliable known trick: it turns "the hole on the left" into "face 7".
  `tags.rs` already does the colour version of this; §4 is the rest of it.
- **A depth channel improves size and distance perception**
  ([SpatialVLM][spatialvlm], [DepthLM][depthlm]) — for photographs, where depth
  has to be estimated. We have the depth buffer exactly, which makes it a probe
  rather than an image; see §3 and §9.
- **Letting the model render its own diagnostic is worth more than more of
  ours.** [Whiteboard-of-Thought][wot] reached 92% on spatial tasks where
  chain-of-thought scored 0%, purely by letting the model write plotting code
  and look at the result.
- **A text grid is the worst of both.** [ViTC][vitc] measured GPT-4 at **25.2%**
  recognising a single character rendered as ASCII art and **3.3%** on two- to
  four-character strings, with chain-of-thought barely moving it;
  [ASCIIEval][asciieval] and [ASCIIBench][asciibench] replicate it. §10.

---

## Where we stand

| capability | parcad | note |
|---|---|---|
| Numbers and pictures in one reply | ✅ `evaluate_part` | `Snapshot` as text *and* structured content, then the PNGs — looking costs no extra call |
| Measured dimensions, volume, area | ✅ `PartReport`, `measure.rs` | tight `bounds`, never `framing_bounds` |
| Multi-view contact sheet | ✅ `render::contact_sheet` | seven orthographic views, one shared framing |
| Scale bar on every panel | ✅ `render::ScaleBar` | round 1-2-5 lengths, end ticks — the "how big is this" answer without a call |
| Region colouring with a legend | ✅ `tags.rs` | which tag owns which surface, drawn on the image |
| Edge listing with geometry | ✅ `list_entities` | centre, direction, length; sampled, with the total |
| Treatment target preview | ✅ `inspect_treatment_target` | plus tags whose edge set is *exactly* the target |
| Selector syntax check | ✅ `check_selector` | no geometry touched |
| Depth + normal per pixel | ~ `render::GeometryBuffer` | exists, and `model_point` already ties a pixel to a millimetre — not exposed |
| Point and ray probe | ✅ `probe.rs`, `probe_part` | §3 — signed distance at a point, every crossing along a ray, and the wall thickness between them |
| Wall thickness / minimum feature | ✅ `thickness.rs`, `measure_wall_thickness` | §5 — a ray from every sampled surface point, both faces named; optimistic where a fillet was dropped, and it says so |
| **Overhang and printability** | ❌ | §6 |
| **Section view** | ❌ | §7, and OP_ROADMAP §8 |
| **Numbered marks on the render** | ❌ | §4 |
| **Diff render** | ❌ | §8 |
| **Face adjacency as text** | ❌ | §9 — we list edges, never faces |
| Adaptive slice summary | ❌ | §10, the salvaged form |
| ASCII / voxel dump | ❌ | §10, hold, with numbers |

---

## 1. The shape of a perception tool

Every entry below obeys the same three rules, which are already how
`evaluate_part` behaves and are worth writing down before they get diluted:

1. **One call answers the question.** `PartReport` exists so nothing has to ask
   a follow-up; a probe that returns a distance but not the point it hit forces
   the second call the first one was supposed to prevent.
2. **The answer names a location.** "Minimum wall is 0.8 mm" is half a tool.
   "Minimum wall is 0.8 mm at (12, −4, 20), between the bore and the outer
   face" is one, and it is the same standard as `OcctError::Crashed` naming the
   fix.
3. **Nothing is inferred that could be measured.** This is the whole page.

### How to tell whether one works — run the field test

**A perception tool is not finished when its number is right. It is finished
when a model reads the number right, and those are different days' work.** The
probe in §3 passed its Rust tests on the first run and then failed three
separate ways in front of an actual model — a flag read inverted, a field name
read as the wrong noun, and the tool not being called at all. None of those is
reachable from inside the process, and no test in this repo can catch any of
them. So before calling anything below done:

```bash
cargo build -p parcad-app --bin parcad-app     # the build you mean to test
PARCAD_OCCT_WORKER=$PWD/target/release/parcad-occt-worker ./target/debug/parcad-app &
tools/field-test.sh eval/field/does-the-port-meet.md 4
THINK=0 tools/field-test.sh eval/field/does-the-port-meet.md 4
```

That is four trials of `claude -p` on Haiku 4.5 in parallel — a separate
process, its own context, every local tool denied so it cannot open the file and
read the answer — against the MCP server the running app hosts. `THINK=0` is the
non-reasoning arm and is worth running: reasoning did not prevent any of the
three failures above, and one of them appeared *only* without it.
`tools/field-test-score.py` prints who called the tool and who quoted a measured
value; the failures themselves have all been in the prose, so open the
transcripts.

Four things this cost to learn, all of which will otherwise cost it again:

- **The app serves the binary it started with.** Rebuild *and restart* between
  rounds. A round that silently tested the old build is indistinguishable from a
  round where the change did nothing.
- **Trials are parallel and independent.** The server is stateless and a script
  carries its whole part, so four at once take one trial's wall clock.
- **Extended thinking is on unless you turn it off,** so an unconfigured run
  measures the reasoning model only.
- **Read the transcript, not the verdict.** The single most useful trial on
  record got the *right* answer while quoting the part's own source comment as
  its evidence. Correct and worthless, and only the transcript says so.

`eval/field/README.md` has what makes a prompt worth adding. Write the result
into the section it bears on, next to the design decision it changed — that is
what §3, §4 and the order at the bottom of this page are now made of.

## 2. Fewer views, chosen — a correction to the default

The contact sheet ships all seven of `View::ALL`. That was the right call for a
person, who scans a sheet in one glance, and is probably the wrong default for a
model, given [OpenECAD][openecad]'s result that four views scored below three.
Back, left and bottom are usually redundant with their opposites for a part that
is roughly convex, and each one costs tokens and attention.

**What it takes.** Nothing in the kernel — `parse_views` already takes a list.
The change is the *default*: `iso, front, top, right` for an agent, all seven on
request, and the tool description saying which to ask for and why. Cheap, and
the sort of thing that is invisible until measured.

**Worth measuring rather than assuming.** This is a claim about our renders, not
theirs. An eval that asks the same question of the same part at four views and
seven would settle it.

## 3. Point and ray probes — **DONE**

**What it is.** `crates/parcad-core/src/probe.rs`, reached as `probe_part`:

- `distance_at(points)` → the signed distance at each. The sign alone answers
  "is this point inside the part", which previously needed a render and a guess.
- `ray(origin, direction, max)` → every crossing along the line, in order, plus
  `solid_mm` and `first_solid_mm` — the total material and the first complete
  run of it.

**Why it mattered.** Two crossings on one ray *is* a wall thickness, measured,
with no picture in the loop. It is the answer to the whole class of question a
render provokes and cannot settle: how thick is that boss wall, does the
counterbore break through, is there material between these two pockets.
[CADSmith][cadsmith]'s gap-at-the-joint failure is one ray cast.

**What it cost, and what was learnt.** The estimate above was right that the
work was small and wrong about the shape of the answer in three places.

*Bisect on the sign, not on the distance.* The field is exact for primitives and
cheap booleans and an **under**-estimate at corners by construction (OP_ROADMAP
§1 — an overestimate deletes geometry, because the octree prunes on it, so every
corner reads short). A sphere trace on such a field converges and never
overshoots, but it also never lands: it approaches the surface asymptotically.
So the march floors its step at `EPS`, which lets a sign change happen, and then
bisects on the sign. That matters more than it sounds: a *crossing position* is
found from the sign, which is exact, rather than from the magnitude, which is
not — so thicknesses are correct even where the field around them is
conservative. A distance reported at a point stays a lower bound, and says so.

*A ray that grazes a surface can march forever.* It reads a near-zero distance
and advances `EPS` a step. That is a real geometric situation, not a bug, so it
is capped and reported (`incomplete`) rather than hidden. Past the last crossing
the answer is *unknown*, which is not the same as *nothing there*.

*The field has no fillets, and the report has to say so.* This was the one that
would have shipped a wrong number quietly. `drawable()` already replaces every
`Fillet` and `Chamfer` with an identity so a part can be drawn at all — a probe
must go through the same door, which means it measures the **sharp** corner:
material the real part does not have. `omitted_treatments` names every treatment
that was dropped, exactly as a region map does. A probe near a rounded edge that
did not say this would be the "valid-looking wrong answer" the whole corpus rule
exists to catch.

*A caller that gives no length means "all the way through".* The default reach
is derived from the origin and the framing bounds and reported back as
`max_distance_mm`, rather than making the caller compute it from a snapshot —
the follow-up question `PartReport` exists to prevent.

**Measured, not assumed.** The unit tests pin closed forms: a 40 mm cube shelled
to 5 mm reads a 5.000 mm wall and 10 mm of material across two walls; the
service tests use the 40 mm plate with a Ø12 bore, where the wall beside the
bore is 14 mm. On real generated geometry — `examples/hex-standoff.js`, 5.5
across the flats, 2.5 tap drill — a ray across a flat crosses at ±2.75 and
±1.25 and reports `first_solid_mm` 1.4999993 against a closed form of exactly
1.5, a ray down the bore finds nothing at all, and the point at the origin reads
+1.25 from the bore wall.

**Driven by a small model, which is the test that matters.** Haiku 4.5 over MCP,
asked for the wall between the bore and a flat on `examples/hex-standoff.js`
without being allowed to compute it from the source: it read the project,
evaluated, fired one ray from the bore wall at a flat, and reported 1.5 mm from
`first_solid_mm`. The same question with `probe_part` withheld got the same
number — *derived* from `acrossFlats` and `tapDrill` in the script, presented in
the language of measurement, at "99% confidence". That is the failure mode the
tool exists to remove: not an unreachable number, but a computed one wearing a
measurement's clothes. On a part where the source and the built geometry have
diverged — the thing `eval/cases/` exists to catch — the derivation is
confidently wrong and nothing in the answer says so.

**And the limit it found: a probe says whether there is material, not what it
is in.** Asked to prove that `manifold-block.js`'s drop ports really meet the
main gallery, the same model measured correctly — port void from z=20 to z=−5,
gallery from z=+4 to z=−4 — and concluded they *did not* meet, inventing a
millimetre of material between −4 and −5 that its own ray had just measured as
void. It read two overlapping intervals as two adjacent ones.

**Re-run with tagged crossings, and the result is worse than a null.** Four
trials, `claude -p` on Haiku 4.5 against the running app's MCP, one prompt: does
a drop port break into the gallery, every number from a measurement. Three never
called `probe_part` at all — two answered from `portDepth` arithmetic (one
quoting the script's own comment as evidence), one from looking at a front view.
The fourth probed four times, was handed `tag: "ports"` on every crossing, and
answered **DO NOT MEET**.

It failed on the sign. Given `{"distance_mm": 5, "inside": false}` at the port's
axis at gallery height it wrote "the material is solid with 5 mm of solid
material remaining", and given `{"distance_mm": -0.5, "inside": true}` a
millimetre lower it wrote "inside a void" — `inside` read as *inside the void*,
exactly inverted, and then reasoned impeccably from it. It had also fired a ray
along +X from that point, got zero crossings over 50 mm, and never used it.

Three things this settles, none of them the thing it was meant to test:

- **A field nothing reads is not a feature.** The tag is correct and was in
  front of the model in the one trial that could have used it. Naming a crossing
  does not help a caller that never fires the ray, and does not survive a caller
  that has the polarity backwards before it starts.
- **`inside` is the bug.** Inside *what* is genuinely ambiguous on a part made
  of negative space, and a boolean gives a model a coin to flip. A string it
  cannot inject a sign error into — `medium: "material" | "void"` — is the same
  information with no free parameter. The same goes for `starts_inside`,
  `ends_inside` and `entering`.
- **`probe_part` is not being reached.** Three of four reached for renders and
  the source instead. That is a tool-description problem, and no amount of
  work inside the tool fixes it.

**Both were fixed, and the fix was measured.** Nineteen trials over three
rounds, half of them with extended thinking off, same prompt throughout:

| round | what changed | reached `probe_part` | correct |
|---|---|---|---|
| 1 | tagged crossings | 1/4 | 3/4, all but one by not measuring |
| 2 | `medium` replaces `inside` | 2/7 | 3/7 |
| 3 | `surface_of` replaces `tag`, plus the tool description | **8/8** | **7/8** |

Round 2 killed the sign error outright — no trial in either arm misread
`medium`, and none has since. It exposed the next one in the same place: handed
`{"into": "material", "surface_of": "ports"}` under its old name `tag`, a model
wrote "crosses into **port material**", taking the tag for the name of the stuff
on the far side rather than the face it went through. Same shape of mistake as
`inside` — a field name that lets the reader supply the wrong noun — and the
same fix, a name with only one reading.

Round 3's other half is that the tool description now says *this is the tool for
'do these two bores meet', reach for it before you reason from a dimension in
the source*, and 4 of 4 did. That single paragraph moved the number more than
either field did. **The most valuable change to a perception tool was not in the
tool.**

Two things the rounds settled that no unit test could have:

- **Reasoning did not help.** Every failure mode appeared with thinking on, and
  round 3's only wrong answer is from the *reasoning* arm while its
  non-reasoning arm went 4/4. Run both; do not assume the smarter setting is the
  safe one.
- **A right answer is not evidence.** Round 1 scored 3/4 correct while
  measuring almost nothing — one trial quoted the part's own source comment as
  its proof. `tools/field-test-score.py` prints who *reached* the tool for
  exactly this reason, and had a bug that scored "DO NOT MEET" as a pass, which
  is the same failure one level up.

**What round 3's one wrong answer asks for next.** It measured 13 mm of material
between the gallery and a port and was right — at z = 16, near the top of the
block, having decided that was where the gallery was. It is at 0. Nothing it
could call would say where a *tag* is: `list_entities` gives edges, a region map
needs eyes, and `probe_part` answers about a line you already chose. A model
cannot aim a ray at a feature it cannot locate. `measure::bounds` over one
tagged node's field is the whole implementation — see the order below.

The reasoning error is the model's. The gap is ours: down a port's axis the port
void and the gallery void are the same air, so no axial ray can distinguish
them, and the measurement that settles it is a *transverse* one the model never
thought to fire. (It is decisive — at the gallery's own height the void at
x=−22 spans y=−5…+5, which is the port's Ø10 and not the gallery's Ø8.) A
crossing that names the tag it entered — `ports` rather than `main_bore` —
makes the intersection a reading rather than a deduction, and that is what §4's
cheap half now does: `tags.rs` already answered exactly that question for a
*pixel*, by asking whose field vanishes there, and a crossing point is the same
query. The transverse ray is still one the model has to think to fire; §9 is
the argument for handing it the faces without being asked.

**No `eval/cases/` entry, deliberately.** A case there is a two-backend
geometry comparison — `Observed` is size, volume, area, triangles, topology —
and a probe is neither a geometry nor available on both backends. Pinning these
numbers there would mean widening the corpus schema for one implicit-only tool.
The closed forms are pinned in the unit tests instead, which is where the rest
of the field's own behaviour is checked. If §5 lands and thickness becomes a
part-level property, that is the point to revisit it.

§5 landed, and the answer is still no: a thickness is implicit-only for the
same reason a probe is, and the B-rep backend has no thickness to disagree
with. Widening the corpus schema for a one-backend number would make `Observed`
mean two things. Its closed forms are pinned in `thickness.rs`'s unit tests —
a 40 mm shell, an off-centre pocket with a 2 mm wall on one side and 12 mm on
the other, and a sphere, whose every inward normal is a diameter.

## 4. Numbered marks — the rest of Set-of-Mark

**What it is.** `tags.rs` already colours a render by which tag owns each
surface and draws a legend. That is Set-of-Mark with colours as the identifier.
The literature's version uses **alphanumerics**, and the difference is not
cosmetic: a model can say "7" in a selector, and cannot say "the olive one".

**Why it matters here.** It closes the loop that the rest of the project is
built around. The model sees `7` on a face, reads face 7's metrics in the same
reply, and writes a selector that resolves to face 7 — instead of describing a
face in English and hoping the selector grammar agrees. Compare
`inspect_treatment_target`, which already does this for edges *after* the fact;
marks do it before.

**What it takes.** `render.rs` has `text` and `label` already, and
`GeometryBuffer::model_point` turns a pixel back into a point on the part. The
work is choosing where to put a label so it lands on the region it names and
does not collide — the centroid of the region's pixel mask, nudged inward.

**The rule that constrains it.** A mark is a rendering artefact, valid for one
evaluation, exactly like `edge@N`. It must never look like something to write in
a script, and the tool description has to say so in the same words
`list_entities` does.

**The cheaper half of this is done: a `Crossing` names what it is on.**
`tags::owners_at` asks of three coordinates what `regions_in` asks of a pixel —
whose field vanishes here — so every crossing carries the `tag` of the node
whose surface it is, or nothing where an untagged node or a fillet owns it.
`crossings_tell_two_voids_that_meet_apart` is §3's manifold, measured: across
the part at the gallery's height the void is bounded by `port` on both sides,
which *is* the intersection, where the same fact as a pair of diameters was a
deduction a model got backwards. Two surfaces can genuinely meet at a point — a
bore's wall and the face it breaks out of, at the rim — and there the nearer
wins; that ambiguity is real rather than a rounding choice.

Marks on the render are the same idea for a viewer with eyes; this was the
version for one without.

**Cost.** Medium, for the marks that remain — still the highest-value *visual*
change.

## 5. Wall thickness and minimum feature

**What it is.** The smallest amount of material anywhere, and where it is. The
standard formulation is the maximal inscribed sphere: at a point on the surface,
march inward along the inverted normal until the field turns, and the distance
travelled is the local thickness.

**Why it matters here.** It is the check every part in `examples/` silently
assumes and none of them verify, and it is the thing a 3D print or a casting
actually fails on. It is also what the commercial DFM tools sell as a feature.

**What it takes.** The `GeometryBuffer` gives surface points and normals for
free — one per hit pixel, from all seven views, which is a dense enough sample
to find the minimum without a mesh traversal. Each sample is one ray from §3,
which now exists, so this is that loop.

**And it inherits §3's caveat, more sharply.** The field has no fillets, so a
sampled minimum near a rounded edge is the sharp corner's thickness. For a
*minimum* that is the dangerous direction: the report would be optimistic about
the very feature most likely to be thin. Either sample from the B-rep surface
and probe the field only along the inward normal, or state the omission at least
as loudly as `omitted_treatments` does.

**Report shape.** Minimum, the point, and the two surfaces it lies between —
plus a count of how many samples fell below a caller-supplied threshold, so
"one bad spot" and "the whole wall is thin" are distinguishable.

**Done**, as `thickness.rs` and `measure_wall_thickness`, in the shape above.
The loop is `probe::rays` — added alongside it, because compiling the field
once per ray is fine for the handful a caller fires by hand and ruinous for the
thousands this fires at one part. Both faces are named through `tags::owners_at`,
the same call §4's crossings use, so the answer reads "5 mm between `block` and
`port`" and the wall is identified rather than located.

Three things the implementation settled that the design above did not:

- **The omission is stated as prose, not as a list.** `omitted_treatments`
  carries the node indices as everywhere else, and a `caveat` string next to it
  says *which way the error runs*: the sharp corner has more material, so the
  reported minimum is an upper bound and the true one is at or below it. A list
  of indices makes an answer vaguer; only this one makes it optimistic, and the
  field name `omitted_treatments` cannot say so. The B-rep alternative is still
  the real fix, and is still not done.
- **Surface samples need refining before they are surface samples.**
  `model_point` reads back a *quantised* depth, so it lands within a voxel of
  the surface — far enough out that the inward ray starts in void, or far enough
  in that every wall reads short by the same bias. Two Newton steps along the
  gradient close it. But a field built from `abs` or `sqrt` has no derivative
  where it is exactly zero, so a point landed perfectly on the surface returns
  `NaN`: success and failure look identical. The last *usable* normal is the one
  kept, never the last one evaluated.
- **A ray thickness is not an inscribed sphere**, and the difference is signed.
  They agree on a wall with parallel faces; in a concave corner the ray crosses
  to whatever is straight across, which is further than the sphere that fits.
  Upper bound again, and stated in the module rather than discovered later.

**Measured on a model**, `eval/field/how-thin-is-it.md`, four trials of Haiku
4.5 with thinking on, against the flange: *what is the thinnest material in this
part, and between which two surfaces?* The ligament between a bolt hole and the
OD is 6.3 mm, and `thickness = 19.1` is sitting in the script one line away from
being the wrong answer.

| reached the tool | quoted a measured value | carried the caveat | correct |
|---|---|---|---|
| 4/4 | 4/4 | 4/4 | 4/4 |

Nothing derived it from the script, and every trial reproduced the caveat *with
its direction* — "the true minimum is at or below 6.30 mm". That is the first
time a warning in a payload has been read back correctly on the first round,
and the paragraph in the tool description saying *use this before you call a
part ready to print* is the likeliest reason, as it was in §3 round 3.

**And the transcripts say the tag names are not enough**, which the score does
not. All four named the pair `plate` / `drilled` and then explained it in
English, and two of the four explained it wrongly: trial 4 put the wall between
"the top surface of the flange" and the bolt holes, trial 1 between the plate
and "the bore". It is neither. The wall is *radial*, from the OD to a bolt hole,
and both trials had the coordinates that say so — `at` and `opposite` share a z.

**The round after it was void, and that is worth recording too.** Rerunning
`does-the-port-meet.md` against the rounded replies, two of four trials never
answered: stuck, they went hunting for a shell, found `Monitor` and `Skill` —
neither on `field-test.sh`'s deny list, which predates them existing — and spent
the run trying to fix this repo's compiler warnings. The scorer counted them as
trials. A deny list is wrong by default every time the CLI grows a tool, so the
scorer now prints a `stray` column instead of trusting the list; eval/field's
README has both failure modes. The §5 round above is unaffected — all four of
its trials called nothing but `ToolSearch` and parcad.

The cause is that a tag names a *node*, not a face. `plate` is one cylinder and
owns the OD, the top and the bottom; `drilled` is one cut and owns the bore and
all four bolt holes. `surface_of` therefore narrows the answer to a handful of
faces and stops, and the model fills the rest in from the part it is imagining.
Same shape as every failure on this page: the number survived and the
*location* drifted. It is an argument for §9 rather than a defect here —
nothing short of naming faces can say "the OD" — and the interim mitigation is
that the coordinates are already right and already in the reply.

**Replies are rounded to the micron**, in `service::round_mm` and nowhere else.
Not a size optimisation first — though it is a quarter to a third of every
numeric reply — but the same rule as the rest of this page pointed at
precision. The field is evaluated in f32, and an f32 widened to f64 has no short
decimal form: 30.15 serialises as `30.149999618530273`, because `serde_json`
must print enough digits to round-trip the f64 it was handed. Those fourteen
trailing digits are the f32's own rounding error presented as measurement, at a
precision three orders past anything the pipeline resolves — and a centroid of
`6.066550368146516e-7` is a zero that reads as an offset. A model has already
been observed spending tokens reconciling the noise: *6.30 mm (measured as
6.29999268054804 mm)*. An f32 field needs none of this; only the widened ones
do.

Re-run against `does-the-port-meet.md` afterwards, four trials of Haiku 4.5:
4/4 reached the tool, 4/4 quoted a measured value, 4/4 correct, none strayed.
Rounding a reply is still a change to a reply, and this page's rule is that the
model reading it is a separate fact from the number being right.

## 6. Overhang and printability

**What it is.** A per-face angle test against the build direction.
[AgentsCAD][agentscad] uses precisely the deterministic rule you would write by
hand — flag any face whose normal makes too shallow an angle with the build
plane — plus radius of gyration as a footprint proxy and an elongation index for
how lopsided that footprint is. No model judgement anywhere in the measurement;
the model's job is deciding what to do about it.

**Why it matters here.** Same argument as `METRIC_FASTENERS`: this is standards
knowledge currently hiding in an author's head. And it is the natural home for
the thread annotation OP_ROADMAP §5 wants — a report that knows a hole is
tapped M6 is a report that can check the boss around it is thick enough.

**What it takes.** Small on the B-rep side, where faces and their normals are
already available. `mass_properties` in `measure.rs` gives the inertia terms
that the gyration and elongation numbers come from.

**Where the line is.** Report the geometry, not the verdict. "Face 12 overhangs
at 18° from the build plane" is a measurement; "this part will fail to print" is
a process opinion that depends on a machine we know nothing about.

## 7. Section view

Already on OP_ROADMAP as §8, filed as a view concern rather than an op, and it
is the single thing most missed while writing the corpus (DSL_GAPS §0). It
belongs on this page too, because for an agent a section is not a convenience —
it is the only way to see an internal feature at all. A cut through a boss is
worth ten isometrics, and `intersect(part, box(...))` is the wrong answer
because it produces a *different part*.

**What it takes.** A clipping plane in the raster: reject any hit before the
plane and shade the cut surface flat. `render_view` already walks depth along
the view axis, so the plane is a start offset plus a cap colour.

**Cost.** Small, and it serves the window and the agent with one change.

## 8. Diff render

**What it is.** The same camera, the same framing, before and after one edit,
plus the numeric deltas — volume, bounds, face and edge counts, tag set.

**Why it matters here.** Models are markedly better at "what changed" than at
"what is", and the question an agent asks after every edit is the former. It is
also the cheapest possible guard against the silent failure this project keeps
running into: an op that runs, returns a valid solid, and changes nothing.

**What it takes.** Small. Two evaluations, one shared `bounds` for framing (the
existing `framing_bounds` is exactly the right thing to share), and a
subtraction. The numeric half is worth shipping even without the images.

## 9. Faces as text

**What it is.** What `list_entities` does for edges, done for faces: surface
type, area, centroid, normal, and an `adjacent_to` list of face ids. This is
where the CAD-specific literature has converged — [BrepLLM][brepllm],
[Pointer-CAD][pointercad] and [AgentsCAD][agentscad] all serialise the
face-adjacency graph as text and feed it alongside the render.

**Why it matters here.** It is far denser per token than any image, it survives
the model having no vision at all, and it is the natural key for §4's marks.
Selectors today reach edges; a part whose *faces* have names is the precondition
for per-face offset and for draft applied to an existing face
(OP_ROADMAP's "whole-body offset, not per face").

**Cost.** Medium, and it interacts with the selector grammar — which is parsed
twice on purpose, so a face selector is a change in `selectors.rs`,
`selectors.ts` and `eval/selectors.json` together, or it is a red test.

**§5's field round is the strongest evidence for it.** A tag names a node, and
a node owns several faces — `plate` is the flange's OD *and* its top *and* its
bottom. Handed a correct thickness between `plate` and `drilled`, half the
trials described the wrong wall, because the name they were given cannot
distinguish the faces it covers. Every measurement that reports *where* runs
into this, and no amount of care in the measuring tool fixes it.

## 10. Slices — the idea, and the version worth building

**The idea as proposed** was a stack of ASCII grids, one character per
millimetre, a centimetre apart. It should not be built in that form, and the
numbers are unambiguous: [ViTC][vitc] puts GPT-4 at 25.2% on *one* character
rendered as ASCII art and 3.3% on short strings, and prompting does not rescue
it. Tokenisation destroys column alignment before attention ever sees the grid.
A structured slice is easier than ASCII art — regular cells, a legend we
control, no artistic ambiguity — so it would do better than 3%. But the
questions it would be for (is that hole round, is that wall 2 mm or 3 mm) are
exactly the fine-grid perception that fails, and §3 answers them exactly.

The cost seals it: a 100 × 100 mm part at 1 mm cells is 10 000 characters *per
slice*, so twenty slices is roughly 60 000 tokens for what a contact sheet plus
a report already answers in three.

**What slices are genuinely best at**, and nothing else on this page covers:
counting at a known height, connectivity (is this level one solid or two), and
the Z at which either of those *changes*. So keep the axis scan and throw away
the raster:

```
z=0.0    solids=1  area=1840mm²  x=[-30,30] y=[-15,15]
z=12.0   solids=1  area=842mm²   holes=2  ⌀5.0@(-20,0) ⌀5.0@(20,0)
z=13.0   solids=2  ...                      ← the boss splits here
```

Forty tokens instead of ten thousand, and the transitions are the interesting
part — so choose the heights adaptively at topology changes rather than every
centimetre. `sdf.rs` gives the occupancy test and `tags.rs`'s region pass
already does connected-component work on a pixel mask; a slice is that mask
taken on a plane instead of a view.

**Cost.** Medium, and it should wait behind §3 and §5, which answer most of what
motivated it.

## 11. Let the agent render its own — hold, but not for long

[Whiteboard-of-Thought][wot] is the strongest result on this page: up to 92% on
tasks where chain-of-thought scored 0%, from nothing but letting the model write
plotting code, run it, and look at the output. The parcad version is a script
that returns a *plot* rather than a solid — a thickness histogram, a profile
curve, hole centres as an XY scatter — instead of only the cameras we chose.

It is a hold because the sandbox boundary is a hard rule: agent scripts run in
`script.rs`'s QuickJS sandbox, never in the webview, and a plotting library is a
new dependency inside that sandbox with a new set of things it can reach. The
narrow version — a `plot(points)` export that hands data back to the *host* to
rasterise with `render.rs`'s existing primitives — keeps the boundary intact and
is most of the benefit. Worth doing after §9, which is what would supply the
data.

## 12. Depth maps as an image — hold

[SpatialVLM][spatialvlm] and [DepthLM][depthlm] show a depth channel measurably
improving size and distance perception. Both are working from photographs, where
depth is *estimated* and the map is genuinely new information. Here the depth
buffer is exact and already in hand — which means the useful form of it is a
probe that returns a millimetre (§3), not a greyscale image of it for a model to
squint at. Shipping the picture would be re-encoding a number as pixels, which
is the inverse of this page's rule.

---

## Suggested order

1. ~~Point and ray probes~~ — **done**, measured against closed forms, and it
   found the treatment-blindness caveat that §5 now inherits.
2. ~~Name what a crossing hit~~ (§4, the cheap half) — **done**. `tags.rs`
   already had the query; it took a field on `Crossing` and one bulk call.
3. ~~Say `material` or `void`, not `inside`~~ — **done**, with `surface_of`
   for `tag` and a rewritten tool description alongside it. Measured: 1/4 of
   trials reached `probe_part` before, 8/8 after. The description did more of
   that than either field.
4. **Where is this tag?** (§3, from round 3's only wrong answer). Bounds and
   centre of one tagged node's field — `measure::bounds` on a tree `tags.rs`
   already lowers, so it is an afternoon. A model that cannot locate a feature
   aims its rays at the wrong plane and measures something real, correctly, in
   the wrong place, which reads exactly like a right answer.
5. ~~Wall thickness~~ (§5) — **done**. It was §3 plus a loop, once the samples
   were walked onto the surface; the caveat needed a sentence rather than a
   list, because this is the one omission that reads *optimistic*.
6. **Section view** (§7). Already wanted by the window; the agent needs it more.
7. **Numbered marks on the render** (§4, the visual half). The change with the
   best evidence behind it, and it makes the selector loop closeable.
8. **Diff render** (§8), numeric half first.
9. **Faces as text** (§9). Larger, touches the selector grammar in two
   languages, unlocks per-face work later.
10. **Adaptive slice summary** (§10) and the default view set (§2), both worth
   measuring before building.
11. ASCII grids and depth images: not at all, for the reasons recorded above
   rather than the intention.

Each of these needs a case in `eval/cases/` that pins the *reported* numbers, on
the same argument as everywhere else: a perception tool that quietly starts
describing an older part is worse than one that is missing.

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

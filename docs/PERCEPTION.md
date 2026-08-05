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
| **Wall thickness / minimum feature** | ❌ | §5 |
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

**No `eval/cases/` entry, deliberately.** A case there is a two-backend
geometry comparison — `Observed` is size, volume, area, triangles, topology —
and a probe is neither a geometry nor available on both backends. Pinning these
numbers there would mean widening the corpus schema for one implicit-only tool.
The closed forms are pinned in the unit tests instead, which is where the rest
of the field's own behaviour is checked. If §5 lands and thickness becomes a
part-level property, that is the point to revisit it.

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

**Cost.** Medium, and it is the highest-value *visual* change.

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
2. **Wall thickness** (§5), immediately after, since it is §3 plus a loop and it
   is the check every example silently assumes.
3. **Section view** (§7). Already wanted by the window; the agent needs it more.
4. **Numbered marks** (§4). The visual change with the best evidence behind it,
   and it makes the selector loop closeable.
5. **Diff render** (§8), numeric half first.
6. **Faces as text** (§9). Larger, touches the selector grammar in two
   languages, unlocks per-face work later.
7. **Adaptive slice summary** (§10) and the default view set (§2), both worth
   measuring before building.
8. ASCII grids and depth images: not at all, for the reasons recorded above
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

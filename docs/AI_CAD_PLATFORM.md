# CadQuery-grade CAD, with an AI-native operating environment

## Decision

Parcad should target **CadQuery-grade parametric modeling coverage** plus an
agent environment that can inspect, change, verify, compare, and explain a
model. CadQuery is a reference for capability and interaction design only;
Parcad must not depend on, embed, or execute the CadQuery codebase.

The product is not "a smaller CAD DSL." It is an intent-preserving CAD system
whose primary user may be an agent, but whose work must remain legible and
controllable by a person.

Parcad's reason to exist is not a longer operation catalogue than CadQuery or
Fusion. It is a faster, safer loop between a person's spatial intent, an
agent's proposed parametric edit, and evidence that the resulting part still
does what was intended. Sketching is the first place that loop must feel
native: a person can draw, point, and speak while the agent keeps a
constrained, inspectable model up to date.

CadQuery is the capability benchmark and a source of proven modeling and
selection patterns. It is not a runtime dependency, a compatibility target, or
a reason to duplicate its implementation or every surface detail of its Python
API.

## Strategic options

### Option A — fork CadQuery and add AI tooling

Fork CadQuery, retain its Python/OpenCascade modeling stack, and build ParcAD's
agent experience around it: visual inspection, structured evaluation results,
agent tests, code-to-viewport links, controlled experiments, and safer
execution.

**Upside:** the fastest route to broad, proven CAD capability; existing
CadQuery examples and idioms work with little translation; the team can invest
primarily in agent tooling.

**Cost:** ParcAD inherits a Python-centric fluent API and a large external
codebase as a permanent core dependency. The intent graph, dual SDF/B-rep
evaluation model, topology protocol, and agent sandbox must adapt around that
runtime. Fork maintenance also becomes a product responsibility.

**Use this option when:** time to broad mechanical-CAD coverage matters more
than owning the modeling architecture and language.

### Option B — ParcAD-owned CAD kernel and AI tooling

Implement CadQuery-level capabilities over ParcAD's own intent graph and
OpenCascade-backed execution layer. Use CadQuery only as a behavior,
capability, and regression-test reference.

**Upside:** one native model contract for exact geometry, SDF perception,
topology lineage, tests, and agent tooling. The public language can be designed
for stable provenance and verification rather than inherited from a fluent
workplane stack.

**Cost:** substantially slower path to feature coverage; every major operation
needs bindings, semantics, geometry tests, inspection data, and error handling.

**Use this option when:** independent architecture and an AI-native execution
contract are the product's durable advantage.

### Current choice

Choose **Option B**. Keep Option A visible as the fallback if the delivery cost
of independent feature coverage outweighs the value of owning the modeling
stack. Revisit the choice only with a scoped implementation comparison, not
after accumulating an accidental half-fork or half-clone.

## Product boundary

| Parcad owns | Parcad must provide | Do not make a primary interface |
|---|---|---|
| intent graph, agent protocol, inspection, verification, visual diffs, safe execution | constrained sketches and workplanes; extrude/cut/pull/revolve/loft/sweep; holes, patterns, booleans, shell/offset, fillets/chamfers, imports and assemblies | raw B-rep array indices such as `edge[17]` |
| topology provenance and named feature references | CadQuery-level capability coverage implemented against ParcAD-owned interfaces | permanently serialising a kernel's transient edge handle |
| live sketch interpretation, proposal/review states, and code-to-viewport links | dimensions, topology, mass, thickness, clearance and manufacturability checks | screen-coordinate doodles or opaque direct edits as the only stored design intent |
| perception modes that exploit the SDF and exact B-rep | a clear distinction between a parametric feature edit and a direct-modeling pull | a second, incomplete implementation of every OpenCascade feature |

The exact-modeling provider is ParcAD-owned: direct OpenCascade today, with
additional ParcAD implementations where needed. The public intent graph and
evaluation protocol must remain independent of a third-party CAD runtime.

## Desktop and web parity

The Tauri desktop application and web application are two hosts for one
Parcad product. They must have the same authoring language, intent graph,
feature set, sketch behavior, selection results, history, evaluation
artifacts, LSP experience, and agent protocol. A model authored or modified in
one must reproduce the same accepted result and diagnostics in the other.

Literal implementation identity is neither necessary nor possible: desktop can
offer native file dialogs, local kernels, local models, and OS credential
storage that a browser may not. Those differences belong behind a small,
explicit host-capability interface. They must not produce different CAD
semantics or a separate version of a product feature.

| Shared product layer | Host capability boundary |
|---|---|
| UI feature modules, authoring SDK, parser/LSP, intent graph, selector engine, history projection, viewport interactions, evaluation/artifact schemas, model migration, and agent tools | local/remote evaluation execution, filesystem and import/export handles, credential storage, native dialogs, local-model connection, notifications, and window integration |

Every feature declares its required capabilities. When a host lacks one, the
product presents the same feature with a supported transport or a precise
unavailable state—for example, remote evaluation in the browser instead of a
local kernel—not a subtly different sketch or selection implementation. A
native convenience may be additive; it cannot become the only way to create,
inspect, or verify a CAD feature.

### Parity regression contract

For each representative model and agent task, CI must run the same scenario in
both hosts and compare:

- source/intent graph and migration result;
- operation and selector-resolution reports;
- `SketchDraft` and `EvaluationSnapshot` schema plus key measurements;
- assertions, diagnostics, visual history entries, and code-to-viewport links;
- export/import round trips where the host supports the requested transport;
  and
- deterministic view snapshots within an agreed rendering tolerance.

Feature work is incomplete until it passes this cross-host suite. A temporary
host limitation must be a visible, tracked capability gap with a regression
test; it must never be hidden by a platform-specific fallback. Keep Tauri and
web shells thin, and prohibit parallel implementations of modeling, selection,
or history logic in each shell. This is how the product avoids the current
pattern of features diverging or regressing across targets.

## Strategic focus: where ParcAD should be better

Fusion is already a mature interactive CAD application with a broad feature
timeline. CadQuery is already a productive scripted modeling layer. ParcAD
does not win by copying either one operation at a time. It wins only if the
following capabilities work together:

1. **Strict and conversational visual sketching.** Use ordinary precise CAD
   sketch tools when the geometry is known; alternatively draw an approximate
   profile, point at it, or provide an image and say what matters. Both paths
   produce the same exact, editable sketch and named operation.
2. **Grounded agent actions.** An agent sees the current sketch, B-rep,
   feature lineage, selection result, dimensions, views, and failures—not just
   source text or a screenshot.
3. **Safe, reversible exploration.** Each proposal evaluates in a temporary
   revision, displays a geometry/topology/measurement diff, runs assertions,
   and only then becomes the chosen design state.
4. **Stable intent after change.** Code, viewport selections, named features,
   and semantic selectors refer to the same entities even as later operations
   reshape the part. Ambiguity and selector drift are reported, never hidden.
5. **Geometry-aware verification.** The agent can test fit, wall thickness,
   clearance, volume, topology health, machining/printing rules, and visual
   regressions as ordinary model assertions.

These are an integrated operating environment, not an AI chat panel bolted
onto a CAD API. A Fusion add-in can reproduce isolated pieces of this; ParcAD
should make the full inspect → propose → preview → verify → commit loop its
native contract.

## AI-native sketches and feature creation

A sketch must be a first-class parametric object: plane, geometry, reference
relationships, dimensions, constraints, solver status, and named semantic
regions. A doodle, screenshot, photograph of a napkin sketch, or imported
reference image can all be ways to create or modify that object. The
image-to-CAD experience should feel magical; the resulting model must still be
editable and verifiable.

### Two authoring modes, one model

Parcad must support both of these modes without treating either as second
class:

| Mode | Best when | Interaction | Result |
|---|---|---|---|
| **Strict parametric sketching** | dimensions, relationships, and operation are already known | choose a plane; draw exact primitives; apply snaps, dimensions, and constraints; create the feature directly | an ordinary constrained sketch and named feature |
| **Exploratory AI sketching** | the user has a rough idea, doodle, image, or natural-language description | draw loosely, import an image, point to references, and converse while the agent proposes geometry and constraints | the same ordinary constrained sketch and named feature, after review |

The user can move freely between modes. For example, they can doodle a bracket,
accept an agent's rectangle-and-hole interpretation, then manually lock a
dimension and add a strict concentric constraint. Conversely, they can begin a
precise sketch and ask the agent to add a symmetric mounting pattern or turn a
profile into a revolve. There is no "AI geometry" that becomes uneditable once
accepted.

### Interaction contract

In a live sketch session, the person can combine three inputs without changing
tools:

- **draw** rough lines, arcs, circles, centerlines, and construction geometry;
- **drop in an image** of a hand sketch, annotated photo, or reference part and
  point to the region it should interpret;
- **point or select** a vertex, segment, face, hole, plane, or an existing
  dimension in the viewport; and
- **talk or type**: "make these equal", "this is a 40 mm mounting rectangle",
  "keep it centered", "revolve around this line", or "pull this flange out
  3 mm".

The agent continuously produces a *proposed* constrained sketch or feature and
shows it in the viewport. It should highlight the referenced entities and
state the interpretation in compact CAD terms: for example, "two equal 3 mm
holes, symmetric about the vertical construction line; 30 × 20 mm spacing."
The person can correct the gesture, drag a dimension, edit the generated code,
or say "no, the other circle". Those edits update the same draft rather than
starting a new opaque command.

An accepted result becomes a named sketch plus a named feature in the intent
graph. The feature can be an additive or subtractive extrude, a through-all
cut, a symmetric extrude, a revolve, sweep, loft, pattern, or a semantic
press/pull edit. The source image and interpretation can remain attached as
evidence, but the design authority is normal parametric CAD data—not an
uneditable image, chat transcript, or one-off mesh edit.

### The essential loop

```text
rough doodle + selected references + conversation
                         |
                         v
         proposed sketch geometry and constraints
                         |
              live solver + exact preview
                         |
                         v
       proposed feature: extrude / cut / pull / revolve / ...
                         |
             evaluation, diff, assertions, explanation
                         |
                         v
          accept as named parametric graph operations
```

The agent must never silently promote a low-confidence interpretation. If the
gesture could mean a construction line or a profile edge, it presents both
interpretations. If dimensions conflict, it reports the conflicting
constraints and offers the smallest repair. Until acceptance, a proposal is a
reversible draft; after acceptance, it is a normal editable sketch and
feature, with source-to-viewport links.

### Concrete use cases

**Multi-hole bracket from a loose sketch.** The user doodles a bracket plate
and an upright mounting ear, then says: "make the base 80 by 60 by 4, add four
M3 clearance mounting holes, two 6 mm cable holes, and two 4.3 mm service
holes with 8 mm counterbores." ParcAD creates dimensioned sketches, names the
three hole operations, and uses a different edge treatment for each family:

- the four `mount_holes` top opening rims receive a 0.4 mm fillet;
- the two `cable_holes` top opening rims receive a 0.8 mm fillet; and
- only the two outer `service_counterbores` opening rims receive a 0.6 mm
  fillet—not the inner bore rims or the counterbore-floor edges.

The preview shows each set in a distinct color and reports `4`, `2`, and `2`
resolved edges. This is an intentional selector regression case: adding a
further circular hole must not change any of those three fillet operations.
An illustrative proposed form is:

```js
bracket.edges({
  generatedBy: "mount_holes",
  curve: "circle",
  role: "holeOpening",
  adjacentTo: { faceNormal: "+z" },
}).expect({ count: 4 }).fillet(0.4);

bracket.edges({
  generatedBy: "service_counterbores",
  curve: "circle",
  role: "counterboreOpening",
}).expect({ count: 2 }).fillet(0.6);
```

**Revolved knob.** The user draws half a rough profile and a centerline, then
says: "this is a 360-degree revolve; the grip radius is 18 and keep a 2 mm
wall." The agent proposes a constrained profile and revolve, then checks wall
thickness and reports any self-intersection before accepting the feature.

**Feature-aware pull.** The user selects a flange face and says: "pull this
out 3 mm without moving the mounting holes." ParcAD first explains the two
possible edits: change the flange's originating extrude distance, preserving
the downstream holes; or perform a direct face offset. The preview labels
which path it chose and shows whether hole provenance and selectors still
resolve. Parametric feature edits are preferred whenever they express the
request; direct edits are explicit and remain traceable.

### What the agent receives during the live session

Each sketch update should expose a compact `SketchDraft` artifact rather than
requiring the agent to infer state from pixels:

- sketch plane and local coordinate system;
- primitive geometry, construction state, and hover/debug IDs;
- constraints, dimensions, degrees of freedom, and solver conflicts;
- selected B-rep references and their feature provenance;
- candidate interpretations with confidence and unanswered ambiguities;
- exact preview, section/measurement probes, and before/after diff; and
- a proposed named graph patch plus tests to run on acceptance.

This is the sketch-level counterpart to `EvaluationSnapshot`. It makes a
real-time agent useful while keeping the final model deterministic,
inspectable, and independently editable.

## Bring your own model

LLM choice is an inference configuration, not part of the CAD document format
or modeling kernel. A user should be able to use a hosted model with their own
credentials, a company gateway, or a compatible local model. The same
Parcad-owned inspection and action protocol must work for all of them.

The model adapter needs only a small capability contract:

- streaming text and tool calls for real-time conversation;
- optional image/vision input for doodles, sketches, and viewport snapshots;
- structured output for proposed sketch patches, graph edits, assertions, and
  explanations; and
- declared limits, such as whether it supports images, tool calling, or long
  contexts, so the UI can degrade honestly.

No model receives privileged kernel access merely because it is configured as
the assistant. It requests scoped ParcAD tools—inspect the current revision,
create a proposed patch, evaluate it, read its snapshot, and ask for
acceptance. The sandbox, evaluation artifacts, feature graph, source patches,
and test results are model-neutral and replayable. This preserves privacy and
avoids provider lock-in while allowing different models to be compared on the
same CAD task.

## CAD-aware language intelligence

Parcad should expose an LSP for its authoring language. This is not merely
autocomplete for `extrude()`; it is the editor-facing view of the intent graph
and evaluation results. It lets a person work in a normal code editor and
gives an AI a structured way to read and change the model without trying to
reconstruct meaning from text alone.

### Human authoring experience

- **Completion and signature help** for operations, workplanes, sketch
  primitives, constraints, dimensions, units, feature tags, selector fields,
  and valid values in the current modeling context.
- **Go to definition, references, and rename** across a named sketch, feature,
  parameter, semantic selector, test, and the geometry they produce. Renaming
  `mount_holes` must safely update its generated-by selectors and assertions.
- **CAD hovers** that show documentation plus the current resolved selection:
  count, stable provenance label, dimensions, status, and a viewport highlight.
  Transient debug IDs such as `edge@42` can appear here, but never become the
  default authored reference.
- **Diagnostics at the source location** for invalid units, under- or
  over-constrained sketches, conflicting constraints, failed operations,
  unresolved selectors, selector drift, and failed geometry assertions.
- **Code actions** to add a feature tag, replace a fragile entity reference
  with a semantic/provenance selector, insert `expect({ count })`, repair a
  selector after an evaluation, or navigate from a direct pull to its
  originating parametric feature.

Code and viewport must be one navigation graph: hover or select a line of code
to highlight the resulting sketch or B-rep entities; hover or select geometry
to reveal its feature, source range, parameters, and references in code.

### Agent authoring experience

An agent should use standard LSP capabilities—document symbols, workspace
symbols, hovers, definitions, references, diagnostics, formatting, and
workspace edits—to understand and make small, precise source changes. That
makes agent behavior portable across editors and models.

LSP is not the geometry execution protocol. It should be paired with a small,
versioned ParcAD tool surface for operations that require an evaluated model:

```text
inspect revision / sketch / entity / selector
evaluate proposed revision
resolve selector and return explanation + viewport highlight
preview patch and return geometry/topology/measurement diff
run assertions and manufacturing checks
accept or discard the isolated revision
```

Those tools return the same `SketchDraft` and `EvaluationSnapshot` artifacts
to an editor-integrated assistant, a CLI agent, or any BYOD model. An AI may
use LSP to find `mount_holes`, then ask the evaluation protocol which four
top-hole rims it currently resolves to, make the minimal source edit, and
verify the new result. It must not guess an edge identity from a token offset
or pretend that an editor diagnostic is an exact geometry evaluation.

## Visual feature history

Parcad needs a Fusion-style visual feature history, but it should be an
inspectable projection of the intent graph and evaluation snapshots rather
than an opaque chronological command log. Its purpose is to answer: *what did
this operation change, what does it depend on, and what will break if I edit
it?*

Each history entry represents a named sketch, feature, direct edit, imported
body, test, or agent proposal. Its card shows:

- a small before/after thumbnail or overlay, plus the operation type and
  parameters;
- input entities and source features; created, modified, and deleted topology;
- source location, linked selectors, resolved counts, and pass/fail checks;
- dependents that will be recomputed if it changes; and
- a clear state: accepted, currently selected, failing, suppressed, or a
  proposed revision awaiting review.

The interaction should support timeline scrubbing and hover preview. Scrubbing
evaluates the historical graph state in an isolated revision; hovering a card
highlights the geometry it created or modified; selecting geometry highlights
its origin entry and downstream dependents. Users can compare any two entries
with a geometry, topology, dimensions, and test diff—not merely a visual
animation.

For the multi-hole bracket, selecting the `service_counterbores` fillet in the
history should show only its two outer opening rims. Scrubbing before that
feature shows the counterbores without the blend; scrubbing after it shows the
0.6 mm result; a later added cable hole stays visibly outside its dependency
and selection set. This is the human-facing proof that the semantic selector
means what the code says.

History is also where agent experimentation remains trustworthy. A proposed
agent change appears as a separate, clearly labeled branch with its diff and
assertion results. Accepting it promotes the graph revision; rejecting it
leaves the accepted history untouched. Direct face pulls must likewise state
whether they edited an originating parametric feature or introduced an
explicit direct-modeling operation.

## Representative industry examples and acceptance suite

The platform needs recognisable engineering cases, not only abstract primitive
tests. Each example below is a product benchmark and a regression fixture; it
is **not** a claim that ParcAD certifies a manufactured part or implements an
entire external standard.

| Example | Core CAD workflow | What it proves about ParcAD |
|---|---|---|
| **Machined multi-hole bracket / fixture plate** | strict or doodled sketch, extrudes, through holes, counterbores, fillets and inspection datums | semantic hole-rim selection; different edge treatments stay independent; visual history and code links explain every operation |
| **Revolved flange or pipe adapter** | half-profile sketch, revolve, bolt circle, circular pattern, bores and concentric relationships | agent can turn a rough profile into a dimensioned rotational part; selection follows provenance rather than circular-edge order |
| **Turned shaft, knob, or pulley** | constrained profile, revolve, grooves, chamfers, keyway cut and patterned features | dimensional edits, wall/clearance checks, named parameters, section inspection and source-level rename/refactor |
| **Injection-molded electronics enclosure** | imported/doodled outline, shell, draft, ribs, mounting bosses, snap features and interference checks | model-aware recommendations for wall thickness and draft; the agent must show assumptions and flag manufacturability rules rather than inventing certainty |
| **Sheet-metal control enclosure** | sketches, flange/bend features, cut-outs, patterned fasteners and flat-pattern/drawing output | phased coverage of bends and unfolded states, plus clear feature history from 2D intent through manufacture-ready output |
| **Assembly fixture with a lid, fasteners, and a locating pin** | components, mates, configurations, interference and clearance analysis | cross-part entity references, assembly inspection, and assertions such as lid clearance without fragile per-body edge IDs |
| **Drawn-and-toleranced production part** | named datums, dimensions, tolerance annotations, drawing views and export | an eventual drawing workflow that keeps model, dimensions, and inspection requirements connected instead of treating a drawing as a detached image |

The final two rows belong to the broader-CAD phase; they should be visible
targets now rather than implied support. The standards-facing examples must
name the standard/profile and revision they target. For example, ASME Y14.5
defines the rules and language for GD&T in drawings and digital models, while
ISO 10303 defines the STEP family of product-information exchange standards.
Parcad may validate a declared subset, but must never label an export or a
model "ASME/ISO compliant" without a defined profile and a corresponding
verification suite.

Every benchmark should be represented in four forms:

1. a strict, hand-authored parametric model;
2. a doodle/image plus conversational reconstruction task;
3. an agent change request with expected source, geometry, and history diff;
   and
4. assertions for dimensions, topology, selectors, and relevant clearance or
   manufacturing rules.

This makes the suite a direct comparison of strict CAD authoring, AI-assisted
creation, and safe AI modification. It also prevents a beautiful preview from
masking a broken feature graph or selector.

References:

- [ASME Y14.5 dimensioning and tolerancing](https://www.asme.org/codes-standards/find-codes-standards/y14-5-dimensiones-y-tolerancias/2018)
- [ISO 10303-1 product data representation and exchange](https://www.iso.org/standard/83105.html)

## Why a plain edge index is the wrong foundation

`>X` means "whatever is rightmost in the current result." That is useful when
rightmost is the actual design intent, but it is not an identity.

Likewise, `op#3.edge[2]` is only an ordering convention. A Boolean, fillet, or
parameter edit can split, merge, delete, or reorder its output edges. Fusion's
own `BRepEdge.tempId` is explicitly valid only while the owning body remains
unmodified. CadQuery exposes positional and nth selectors, which are useful
filters but still relative. Onshape's FeatureScript instead offers provenance
queries such as `qCreatedBy(featureId, EntityType.EDGE)`.

References:

- [CadQuery selectors](https://cadquery.readthedocs.io/en/stable/selectors.html)
- [Fusion temporary edge IDs](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/BRepEdge_tempId.htm)
- [Onshape FeatureScript query examples](https://cad.onshape.com/FsDoc/library.html)
- [OpenCascade topology naming and evolution](https://dev.opencascade.org/doc/refman/html/_t_naming_8hxx.html)

## Selection model

The selector system has four intentional layers. Higher layers are more
expressive; lower layers are allowed only when the author accepts fragility.

| Layer | Example | Meaning | Default use |
|---|---|---|---|
| spatial intent | `">X and |Z"` | global position and direction | outside/right-side features |
| geometric intent | `{ curve: "circle", role: "hole", adjacentTo: { faceNormal: "+z" } }` | facts about the current B-rep | all upper hole rims |
| feature provenance | `{ generatedBy: "mount_holes", ... }` | entities generated or modified by a named operation | a particular feature's output |
| ordinal escape hatch | `{ ..., orderBy: "radius", nth: 1 }` | a deliberate tie-break after a strong filter | exceptional cases only |

`edge@42` remains a hover/debug identity for one evaluation only. It must never
be emitted as a source reference.

The desired authoring form is:

```js
const drilled = body.cut(...holes).tag("mount_holes");

return drilled
  .edges({
    generatedBy: "mount_holes",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "+z" },
  })
  .expect({ count: 4 })
  .fillet(0.8);
```

`generatedBy`, `expect`, and ordered selection are proposed API, not all
implemented API. A successful resolution should report the count, entities,
explanation, and visual highlight. A failed expectation must explain what
changed, for example: "the selector now found 6 edges because a new boss also
matches its circle and top-face predicates."

## Architecture

```text
authoring SDK / agent tools
          |
          v
   intent graph + named features
          |
          +------------------------+
          |                        |
          v                        v
 ParcAD exact CAD provider   implicit/perception provider
 (OpenCascade-backed)        (SDF, ray/slice/field queries)
          |                        |
          +-----------+------------+
                      v
     evaluation snapshot + topology lineage
                      |
          +-----------+------------+
          |                        |
          v                        v
   viewport/code inspection   agent tests, diffs and explanations
```

Every exact evaluation should yield an `EvaluationSnapshot` containing:

- evaluated B-rep/mesh and STEP/STL export handles;
- faces, edges, loops and adjacency;
- feature lineage: `created`, `modified`, and `deleted` entities for every
  operation;
- dimensions, bounds, mass properties and topology health;
- operation breadcrumbs and structured kernel failures;
- screenshots, orthographic views and, when useful, SDF perception probes.

The lineage record is the hard prerequisite for durable `generatedBy` and
code-to-viewport references after downstream topology changes. OpenCascade's
topological naming facilities model this as entity evolution; a simple list of
new Boolean edges is not sufficient.

## Agent workflow

An agent should be able to run this closed loop without guessing from source
text alone:

1. **Inspect** — read graph, tags, parameters, topology, views and measured
   properties.
2. **Plan** — identify the named feature or semantic selector to change, plus
   the expected post-condition.
3. **Modify** — make a minimal graph/source edit in an isolated model revision.
4. **Evaluate** — build exact geometry and perception artifacts.
5. **Compare** — show geometry, topology, measurements and screenshots before
   versus after.
6. **Verify** — run explicit assertions and domain checks.
7. **Explain** — report the feature changed, selected entities, downstream
   effects, failures and remaining uncertainty.

The agent-facing test surface should be ordinary code, not prose in a prompt:

```js
expect(part).size([80, 60, 44]).within(0.01);
expect(part.edges(topHoleRims)).count(4);
expect(part).watertight();
expect(part).minimumWallThickness().atLeast(1.2);
expect(part).clearanceTo("lid").atLeast(0.25);
```

## Delivery sequence

### Phase 0 — execution-provider decision

Run a short architecture spike before widening the current OCCT wrapper:

- identify the minimum OpenCascade bindings and ParcAD feature abstractions for
  the first major feature tranche;
- define a provider-neutral operation/result protocol;
- define the desktop/web host-capability interface and one cross-host golden
  model fixture before adding target-specific UI paths;
- use CadQuery examples and behavior as acceptance references, without aiming
  to run existing CadQuery Python scripts;
- preserve the current worker isolation and add a secure script sandbox before
  exposing agent execution.

**Exit criterion:** the ParcAD provider can create a model, return a structured
snapshot, and produce a stable error report through the app.

### Phase 1 — CadQuery-grade everyday modeling

Deliver the operations needed for normal mechanical parts:

- workplanes and 2D sketch geometry/constraints;
- extrude, cut, through-all, revolve, sweep and loft;
- holes, linear/circular patterns and mirrors;
- fillet, chamfer, shell, offset and draft;
- robust transforms, booleans and import/export.

Each operation needs both a geometry test suite and an inspectable operation
record. "The kernel produced a shape" is not sufficient acceptance.

### Phase 2 — provenance, selection and assertions

- name every feature with a source-level tag;
- capture `created`, `modified` and `deleted` topology per operation;
- add `generatedBy`, adjacency, tangent-chain, loop and face-boundary queries;
- add `expect({ count })`, dimensional assertions and selector-resolution
  reports;
- expose hover IDs, source ranges and bidirectional code/viewport highlights.
- project named operations and topology lineage into visual history cards with
  selection highlights, source links, and isolated timeline scrubbing;
- make the multi-hole bracket a golden regression task: three independent
  fillet sets must continue to resolve `4`, `2`, and `2` edges after unrelated
  holes are added.

**Exit criterion:** an edit that adds an unrelated circular boss cannot silently
change a four-hole-rim fillet into a six-edge fillet, and the history view
visibly explains why the correct feature remains selected.

### Phase 3 — perception and agent evaluation

- deterministic multi-view and section renders;
- topology and feature-tree summaries sized for agent context;
- SDF field probes, slice stacks, ray arrays and printability fields;
- visual and numerical before/after diff reports;
- a corpus of parts with expected geometry, tests and ablated perception
  channels.

**Exit criterion:** an agent can diagnose why a geometry or selector test
failed using artifacts, rather than re-reading or guessing at source code.

### Phase 4 — controlled experimentation

- cheap model revisions/branches;
- compare-and-promote workflow for an agent's changes;
- operation-level undo/replay;
- proposed branches in the visual history, with before/after geometry and test
  evidence before acceptance;
- named test suites and manufacturing rules;
- reviewable patches that state intent, evidence and residual risk.

### Phase 5 — broader CAD ecosystem

- assemblies, mates and interference/clearance analysis;
- STEP import with feature-recognition boundaries made explicit;
- drawings, dimensions and export validation;
- reusable feature libraries and agent tools/MCP, after the script sandbox is
  secure.

## Non-negotiable quality bars

- No silently changing edge set. Selection drift is a visible error or warning.
- No raw edge ID in authored source.
- No kernel crash without an operation breadcrumb and reproducible input.
- Every displayed entity has an inspectable origin and current status.
- Every agent modification can be compared against the prior evaluation.
- Tauri and web share product semantics and regression fixtures; platform
  differences are declared capabilities, never divergent feature logic.
- Every major capability has golden geometry tests and agent-level task tests.
- An unsupported operation refuses clearly; it never approximates a believable
  but wrong part.

## Immediate next action

Do the Phase 0 provider spike and design the provider-neutral
`EvaluationSnapshot`, topology-lineage protocol, and desktop/web
host-capability contract. Do not add a large batch of individual OpenCascade
bindings or target-specific UI paths before that decision: either would commit
the project to an expensive divergence.

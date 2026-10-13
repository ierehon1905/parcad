# What to do next, and in what order

Three other documents say what is missing and why: [ROADMAP.md](ROADMAP.md) for
the agent surface, [OP_ROADMAP.md](OP_ROADMAP.md) for geometry,
[DSL_GAPS.md](DSL_GAPS.md) for what the language makes hard. None says which to
do *first across all three*, which is what this file is. It holds order and scope
only, and links rather than repeats.

1. **Ship it** — nobody can run this on a machine that is not this one.
2. **Make it live** — the agent surface works and does not feel like it does.
3. **Go deep, not wide** — be the best tool for precision parts rather than a
   weaker Fusion.

## The queue, as of 2026-09-17

One session at a time, because each rewrites the same core files (`graph.rs`,
`backend.rs`, `dsl.ts`). Done items are removed; `git log` has them.

1. **Two more steps a model has to remember, found the same day.** Both are
   the shape of the print check (landed 2026-09-21; `git log` has it, and
   queue item 5 records what the coin-holder session cost): the mechanism
   exists, nothing makes it happen.
   - **A render is colourless unless asked.** A part carrying materials draws
     grey for an agent and coloured in the window — `materials: true` is
     off by default, and nothing in a reply says the part has any. A model
     therefore cannot see that an accent landed on the wrong body, that two
     bodies came out the same colour, or that a part it dressed looks
     undressed. The snapshot should say a part has materials wherever it is
     measured, and the render note should name the flag; the session that
     added the planter's two materials only passed it because it had just read
     the commit that added them.
   - **A tool's pictures do not reach the user.** Every view already carries
     `markdown` for exactly this, and the server instructions say to paste it;
     the same session drew three views, read all three, sent none, and the
     user asked "SHOW IT". Measure whether a model pastes it
     (`eval/field/`), and consider making the reply's first line say so when
     views were asked for.
2. **Homebrew is published but never exercised.** `publish.yml` renders the
   formula and pushes it to the tap, and `ruby -c` is the only thing that ever
   reads it. Nothing installs it, so a formula that installs the wrong path, a
   service that does not start, or an archive missing the worker would be found
   by the first user rather than by us. In order of what it buys:
   - **Before the tap sees it**: `brew style` and `brew audit --strict --formula`
     on the rendered file, in the same `publish.yml` job that renders it.
   - **After the push, install it for real**: a job on `macos-14` (arm64) and
     `ubuntu-latest` that taps, `brew install parcad`, runs `parcad --version`,
     starts `parcad serve`, and calls `parcad tools` against it — the end-to-end
     check that the archive carries its worker and the host comes up. This is
     the one that would have caught a bundle shipping no kernel.
   - **`brew services`** cannot be started headless in CI in a way that proves
     much; `parcad serve` in the background and one MCP call is the honest
     substitute.
   - Dry-run it locally first (`act`, or the same shell steps by hand against a
     published tag): a blind CI round on a release path costs a release.
3. **ParCAD web: agents through a relay** — live since 2026-09-17 at
   <https://ierehon1905.github.io/parcad/> (the app moves to `/parcad/app/` with
   the landing page, on the first deploy from `web.yml`), relay deployed.
   The playground became ParCAD web, the app in a tab: `parcad-host` compiled to
   WebAssembly (`crates/parcad-wasm-host`, `page.rs`) beside the kernel, the
   same routes and MCP server, and a Cloudflare Worker (`relay/`) an AI client
   reaches the tab through. Field cases over the relay with Haiku: 4/4 SOUND
   (change-the-open-part, did-the-window-draw-it, how-thin-is-it ×2). The
   desktop MCP path measured unchanged (cached evaluate 1.99 ms before and
   after, fresh build 22.8 / 22.9 ms, render 47.5 / 48.2 ms). Left:
   - **One URL for a directory.** A link is per tab, and a listing in the
     ChatGPT app directory or Claude's connector directory is one production
     URL every user connects to. The relay needs a pairing step: OAuth whose
     consent page is ParCAD web itself, issuing a token bound to that browser's
     link. OpenAI's review also wants a verified developer, a privacy policy
     page, tool annotations (done) and test cases; submitting is the owner's
     call, and says it was prepared with an AI assistant.
   - **Finish measuring "show it to me".** On claude.ai (the owner's test,
     2026-09-17) the connector built a brick and put it on the page, but
     "show me it in web" first sent the model off to draw its own viewer. The
     session tools and instructions were reworded to say that showing a part
     means save_project then open_project, and the instructions now fit
     Claude Code's 2048 characters in both hosts — in the tab they did not,
     and the ParCAD web paragraph, appended last, was the part cut off.
     `eval/field/show-it-in-my-browser` measures it, run by
     `bun web/field-tab.ts <label> eval/field/show-it-in-my-browser.md`
     against `vite --mode web` (the host module must be rebuilt to change the
     wording it serves). Before the rewording, over the deployed relay: haiku
     3/4 SOUND without thinking (one trial saved the brick, then set_script
     wrote it over the open planter and said it was showing) and 4/4 with;
     sonnet not run. Left: sonnet before, the whole round after, then rebuild
     and redeploy the site — the live site still serves the old wording, and
     a kernel that builds a fit's reference from the part's cache
     (docs/GOTCHAS.md).
   - **The in-chat viewer in claude.ai**: the connector works there; whether
     the 3D card shows under `open_project` was not checked.
   - **Measure `show-me-the-part` for the web**: a tab gives no render path a
     client can open, so the view's `markdown` is absent and the case, as
     written, cannot pass there.
   - **Phones**: the page's layout collapses the viewport to a strip at 375 px
     (seen 2026-09-17, before this work), and a phone suspends a background tab,
     which drops the link.
   - WebMCP, the item this replaces, is still possible on top: the page's
     tools are the host's, so registering them with `navigator.modelContext`
     is a thin adapter, and nothing else here waits on it.
4. **A measured parts library** — fasteners, bearings, boards, devices, each
   held by eval cases. The library is an extension, outside `dsl.ts`:
   `DEVICES` and `device()` were removed from the language on 2026-10-13 for
   that reason, and come back as library parts with measured radii. The way
   in landed the same day: `importPart(name, params)` and `partParams` (DSL_GAPS
   §0). A model reaches for it unprompted: `eval/field/reuse-the-saved-standoff`
   is 12/12 SOUND on Haiku 5.5, half of them with no example in the folder,
   found through `read_docs` alone, so the server instruction need not name
   it. Left: where library parts live and ship (a seeded folder, like
   `examples/templates/`, is the cheap end) and the parts themselves —
   fasteners and bearings from their ISO tables, as `METRIC_FASTENERS` is;
   devices wait on the caliper below.
5. **What the coin-holder session cost, in order** — reviewed 2026-09-18 from
   three lenses, the order and every sample reply in
   [COIN_HOLDER_REVIEW.md](COIN_HOLDER_REVIEW.md). One session designed a
   3D-printable part in six user turns: every geometric claim measured, every
   mechanical claim guessed, 241 KB of script resent, 11 round trips lost to
   the tool surface, one of them a silently wrong section. In order:
   - ~~a one-day correctness batch~~ — landed 2026-09-18: `deny_unknown_fields`
     on every request struct with the meant field named, `--set` coerced by
     schema, dead viewers expired, the script echo dropped from `open_project`
     and `set_script`, the shadowed builtin named, `rotate` arguments
     validated, `edges` on every treatment with `.expect({ atLeast, atMost })`,
     `tags` each once with `nodes` on the extent, and the selector and fillet
     refusals naming `check_selector` and `inspect_treatment_target`;
   - ~~`project` on every script tool and `edit_part`~~ — landed 2026-09-20:
     `project` (or `"@session"`) and `edits` on all seven script-taking tools,
     `edit_part` built before written and snapshotted, `script_sha256` on
     every reply, and the server instruction saying to send a script once;
     docs/PERCEPTION.md §19 has the bytes per case before and after;
   - ~~`checks` in the returned object, reference bodies never exported, and
     `note()`~~ — landed 2026-09-20: `checks: [...]` beside the bodies, judged
     on every build and first in every reply, with `export_part`,
     `save_project` and a saving `edit_part` refusing a failing check unless
     `allow_failing` gives a reason (the door the print check shares); `.reference()`
     bodies measured and drawn but in no file and no whole-part number; and
     `note()`, carried in the reply as "from the script, not measured";
   - ~~overhang and print files~~ — overhang landed 2026-09-21 with the print
     check; the print files were removed again the next day, as a worse copy
     of what every slicer's auto-orient and arrange already do (docs/ARCHITECTURE.md,
     "The orientation is measured in, not exported");
   - ~~`depth_mm` and `contact_mm2` in `between_bodies`~~ — landed 2026-09-21:
     `depth_mm`/`deepest_mm` on an interfering pair (the widest ball inside
     what they share) and `contact_mm2`/`contact_patches`/`contact_center_mm`
     on a touching one (the common of their two skins, exact), on
     `between_bodies` and `check_fit` alike, with `deeperThan` and
     `contactAtLeast` so a check can assert them; docs/PERCEPTION.md §20;
   - ~~`brief()` with an envelope, judged every build, and a scale rule on
     renders~~ — landed 2026-09-21: `brief({ envelope, budgetCm3, holds,
     gesture, printer, material })` stamped on the graph beside `checks` and
     judged on every build, the envelope in the best of the six axis
     orientations, `brief` in every solid reply (with one line of nudge when
     a script declares none), never a door; and the ruler that was only on
     the contact sheet drawn on every shaded view, with `scale_mm` beside it.
     docs/PERCEPTION.md §21. **A measured `POCKETS` table is still missing**
     and was left out rather than invented — it wants somebody to measure
     some pockets;
   - vocabulary: ~~`not` in the query form~~ — landed 2026-09-21 with the
     compact form's refusal naming the query form and `check_selector`
     (docs/PERCEPTION.md §22). Its field case,
     `eval/field/round-all-but-the-uprights`, is **written and never run**:
     it is an authoring case scored on `input`, and only an after-only round
     can measure it, since the key does not exist on a before host and the
     graph is refused there. Still open: `{ at, cut }`, `MATERIALS`,
     `OBJECTS` with profiles, `fit()`, and a listed `styles` topic;
   - then `compare_variants`, snapshot summaries, and last the mechanics
     witness: `check_motion`, `check_flex`.

   **`{ at, cut }`, scoped (2026-09-22).** Ship the equal-distance form only:
   `{ at: [20, 10], cut: 5 }`, the corner replaced by a straight diagonal 5 mm
   back along each edge, the same slot `{ at, round: r }` already occupies. It
   is not a selector — the corner names itself by its position in the list —
   which is why a section needs it even though `.chamfer()` exists: `.chamfer`
   runs on a *built solid*, and the session that needed this had to keep the
   outline to `inset` it for a skirt (L8). Fusion ships three chamfers (equal
   distance, two distance, distance-and-angle), so `cut: [5, 3]` is the
   natural second form — left out on purpose, and addable later without
   breaking a saved part, since `5` and `[5, 3]` are distinguishable at the
   door. Of Fusion's eleven sketch Modify tools this is the only real gap:
   fillet is `round`, offset is `inset`, scale and move are JavaScript on a
   plain array, and trim/extend/break exist only because you drew geometry
   interactively — a declarative list never draws the overhang.

   **Blocked on a caliper, not on code.** Four queued items all want the same
   physical afternoon, and none should be invented: `POCKETS` (a pocket is a
   size, and `brief({ envelope })` has nothing to name), `fit(kind, nominal)`
   (six unsourced printed clearances in one part, §2.5), `MATERIALS` by
   filament, and `check_flex`'s per-axis material table. `METRIC_FASTENERS` sets
   the standard — every number from a cited ISO table.
   A table of plausible numbers with a source line would fail that standard
   in the one place this repo refuses to. One test coupon printed on the
   owner's own machine unblocks all four and makes the numbers *ours*.

   **Name these edges** — not from the review, found while adding `not`
   (2026-09-22). Every tool here goes *selector → edges*: `check_selector`
   parses a string and touches no geometry, `inspect_treatment_target` needs
   a treatment node that already exists, and `list_entities` hands out
   `edge@12` marked "diagnostic data only". Nothing goes the other way, so
   "I can see the edge I want, what do I write?" has no answer, and the loop
   a model actually runs — build, look, point — ends with it guessing a
   selector or pasting an id the DSL rejects.

   `matches_query(edges, query) -> Vec<bool>` already exists: the borrowing
   matcher `not` needed, which is exactly what a synthesiser wants. So the
   tool is resolve the ids, enumerate candidate queries from the term
   vocabulary, keep the masks that *equal* the wanted set — equality and not
   containment, the rule `EdgeLineage::equivalent_sources` already holds for
   the same reason. A few hundred candidates, each a linear pass, on a build
   that is already cached.

   Two risks, and the second is the design: nothing may match exactly, so the
   reply has to carry the near misses and what each one over- or
   under-selects; and a search returns technically-correct rubbish —
   `{ longerThan: 12.7 }` fits this build and breaks the next. Offer
   structural terms only (`on`, `generatedBy`, `dihedral`, `curve`, `role`,
   `parallel`) and refuse a metric-only match rather than ranking it low,
   which is "refuse rather than approximate" applied to a search. ~1 day plus
   a field case. This is what makes an `edge@N` scaffolding rather than an
   artefact: look, point, get back something that survives a dimension
   change.

### Waiting on a decision

- **How ParCAD web is published.** Pages served the `gh-pages` branch, pushed
  by hand as "ParCAD web from main <sha>" with the pre-push hook off. `web.yml`
  now builds the whole Pages artifact instead — the corpus against the
  WebAssembly worker, ParCAD web at `/parcad/app/`, the landing page (`site/`)
  at `/parcad/`, laid out by `site/pages.sh` — and deploys it with
  `actions/deploy-pages`, on a manual dispatch. Waiting on: Settings → Pages →
  Source: GitHub Actions, then a dispatch, then `wrangler deploy` for the
  relay's new `PAGE_URL`. The hand recipe also ran `web/prebuild.sh`, which needs
  the native kernel; the workflow does not, so the first visit waits out the
  kernel's download until it does.
- **A tag inside a tagged copy.** A tag on a move or mirror now outranks the tags
  inside what it copied (`perceive::face_tags`). The session also let the copy's name beat a
  *feature* tag inside the copy — a mirrored part's bore walls read the copy's
  name, not `bore`. Keep, or let feature tags inside a copy keep their names.
- **`crates/parcad-core/src/occlusion.rs` is MPL-2.0**, a close port of
  fidget-raster's ambient occlusion, disclosed in NOTICE.md. Keep, or rewrite it
  independently if no MPL file should live in `crates/`.
- **Reuse checks deferred ("later")**: whether cargo-dist or GoReleaser should
  replace `packaging/render.py` + `publish.yml` given the separate OCCT worker
  and the Tauri app; whether an existing mesh renderer should replace the
  software rasteriser in `render.rs`.
- **Signing**: SignPath Foundation (free) for Windows, not started. The Mac has
  a Developer ID and `tools/sign-mac-release.sh`, but the certificate is not on
  Leon's Mac, so 0.0.12 and 0.0.13 shipped unsigned. Put it there, with the
  notarisation credentials, before the next release.

### Found by the competitor bench (2026-10-13)

ParCAD, text-to-cad, kernelCAD, Limo and plain build123d built the same parts,
graded by one script on stock OCCT. The briefs, transcripts and grader are in
`research_notes/hands-on/`, which is local and excluded from git.

**Everything the bench found, biggest first.** Score: relative importance out
of 10 — how often a model or a user hits it times what it costs when they do,
effort left out. Impact: ★★★ a model ships a wrong part or a user hits it on an
ordinary part; ★★ it costs a round trip in many sessions or erodes trust in a
number; ★ a stumble now and then. Effort: S a day or less, M a few days, L a
strong-model kernel session. Model: who should do it — Fable at high effort
for new exact geometry or deep kernel work (the OCCT core fix only landed
that way), Opus for host, DSL and reply work. Details are in the bullets below
and in the next two sections.

| # | item | score | impact | effort | model |
|---|---|---|---|---|---|
| 1 | the tag that makes the saucer crawl | 8 | ★★★ | L | Fable, high |
| 2 | `allow_failing` is taken on the model's own word | 7 | ★★★ | S | Opus |
| 3 | `check_motion` | 7 | ★★★ | L | Fable, high |
| 4 | a fillet that ends exactly on a hole's rim is refused | 7 | ★★★ | M | Fable, high |
| 5 | `edit_part` refuses with an empty reason | 6 | ★★ | S | Opus |
| 6 | a list argument sent as a JSON string is refused | 6 | ★★ | S | Opus |
| 7 | field runs measure Haiku 4.5, not the current Haiku | 6 | ★★ | S | Opus |
| 8 | a right part the model cannot verify goes out unvouched | 5 | ★★ | M | Opus |
| 9 | a DSL name as a local is a parse error | 5 | ★★ | S | Opus |
| 10 | `edit_part`'s exact-text `old` misses | 5 | ★★ | M | Opus |
| 11 | `read_docs` has no search | 5 | ★★ | S | Opus |
| 12 | the same signatures fetched every trial | 4 | ★★ | S | Opus |
| 13 | the G2 blend behind `.smooth()` | 4 | ★★ | L | Fable, high |
| 14 | a face's listed area misreads spline faces | 4 | ★★ | S | Opus |
| 15 | tool schemas carry non-standard `uint` formats | 3 | ★ | S | Opus |
| 16 | `fillet(selector, radius)` order | 2 | ★ | S | Opus |
| 17 | `wait_s` on one script tool only | 2 | ★ | S | Opus |
| 18 | `.union()` given an edge selection says "not a function" | 2 | ★ | S | Opus |
| 19 | a vertex treatment's `edges` is not what its `.expect` counts | 2 | ★ | S | Opus |
| 20 | the editor never shows `mixed_corners` | 2 | ★ | S | Opus |
| 21 | the area integral costs a fitted ring 0.76 s | 2 | ★ | M | Opus |

- **A fillet that ends exactly on a hole's rim is refused.** Found by
  @FuruRok comparing OCCT with NVIDIA's usd-brep on X (2026-10-13), and
  reproduced here: a 40 × 30 × 5 base, a 3 mm fin across it, a Ø5 through
  hole at x = 6, and a 2 mm fillet on the fin's concave root edges — so the
  fillet's edge on the floor (x = 3.5) touches the hole's rim at one point.

  ```js
  const base = box(40, 30, 5).at(0, 0, 2.5).tag("base");
  const fin = box(3, 30, 15).at(0, 0, 12.5).tag("fin");
  const hole = cylinder(2.5, 20).at(6, 0, 0);
  return union(base, fin).cut(hole)
    .edges({ between: ["base", "fin"], dihedral: "concave" })
    .fillet(2);
  ```

  The part is well defined: stock OCCT's result has the same volume as the
  hole moved clear (7303.330 mm³), which is what a point contact gives. What is
  wrong is the B-rep. Stock OCCT 8.0.1 builds it `IsDone()` and `BRepCheck`
  marks exactly one sub-shape invalid: the floor face right of the fillet
  (bbox x 3.5–20, z = 5), whose outer wire touches its inner wire, the hole,
  at (3.5, 0, 5) with no vertex there. Every wire, edge and vertex on its own
  is valid; a STEP round trip even reads back "valid", so this survives
  export unnoticed. ParCAD refuses it ("its surface runs into itself … an edge
  meets an edge") and offers 1.94 mm, its largest measured success; the 0.5 mm
  overlap and a hole well clear both build. Two things to fix:
  - **The kernel:** make the face valid instead of refusing. The touch point
    has to become a vertex shared by both boundaries, i.e. the floor face split
    there (one wire passing through the vertex, or two faces), either in
    `ChFi3d`'s trimming of the support face (a patch beside 0001's tangent
    pinch) or as a post-fillet repair in `backend.rs` that finds a face whose
    wires touch and splits it through the contact (`BRepFeat_SplitShape` /
    `BRepAlgoAPI_Splitter` with an edge from the contact to the outer
    boundary). Exact either way, so "refuse rather than approximate" holds.
    Done when the script above builds one valid solid of 7303.330 mm³ that
    passes `BRepCheck` in memory, with an `eval/cases/` case for touch,
    overlap and clear, and the 1.94 mm search no longer needed for it.
  - **The message:** the refusal says "OpenCASCADE's validity check passes
    such a shape"; here the in-memory check fails it (only the STEP round trip
    passes). Say which check caught it.
  Repro scripts, stock-OCCT runs and the renders: `research_notes/smlib-vs-occt/fillet-hole/`.
- **`allow_failing` is taken on the model's own word.** The 0 mm-wall boss
  through OpenCode: GLM 5.3 Flash, Qwen 3.8 Flash and DeepSeek v4.1 Flash each
  passed `allow_failing` with a reason they wrote themselves ("the user fixed
  these exact dimensions") and exported; GLM and Qwen then said DONE. The
  parameter's description says the reason is the user's decision, in their
  words — nothing holds a model to that. Fix candidates: an export written
  under `allow_failing` says so in its first line and in the file's name or
  metadata, and the server instruction says to ask the user before passing it.
- **`edit_part` refuses with an empty reason.** GLM 5.3 Flash, plate brief
  (`oc-glm-qwen/plate-P-glm-5.3-flash-1`): moving the holes produced
  `edit_part refused: . Fix the part and call again, or pass allow_failing` —
  the sentence that names what failed is empty. Find which check reaches the
  refusal with no text and give it its message; an error that names nothing
  breaks the rule every refusal here keeps.
- **A list argument sent as a JSON string is refused.** Qwen 3.8 Flash through
  OpenCode sent `views` as the string `"[\"iso\", \"top\"]"` in 8 calls, and
  GLM sent `points[0]` as `"[[0"`; each is refused with a correct message and
  retried. When a list parameter arrives as a string that parses as that list,
  read it — `parcad call` already coerces `--set` by schema; the MCP door can
  do the same.
- **Field runs measure Haiku 4.5, not the current Haiku.** `field/run-case.sh:19`
  and `field/run-suite.sh:35` pin `claude-haiku-4-5-20251001`; `haiku` now
  resolves to `claude-haiku-5-5`, which built 21/21 right parts on ParCAD where
  4.5 built 23/24 on the easier briefs and 0/2 on the plate stand. Every past
  "small model" number in PERCEPTION.md and the field cases is 4.5's. Default
  to 5.5 and say which in each table.
- **A right part the model cannot verify goes out unvouched.** Haiku 5.5, plate
  stand: two of three ParCAD stands passed every requirement and the model
  still wrote STATUS: PROBLEM — it could not confirm the peg count, the rows'
  spacing or the lean from what the replies give (Sonnet did the same once).
  The verdicts measure printability and fit; nothing answers "are there 18 of
  these, 28 mm apart, leaning 10° outward". The *requirements first* idea
  (New capabilities, 2) is the fix.
- **`read_docs` has no search.** Haiku 5.5 called `read_docs({ query: … })` 8
  times in 2 trials (`haiku55/plate-P-haiku55-1`); the tool takes `topic`,
  `section`, `detail`. Either accept `query` as a search over section names and
  text, or name `section` and the contents list in the refusal.
- **Tool schemas carry non-standard `uint` formats.** schemars writes
  `"format": "uint"` / `"uint64"` on every count; OpenCode logs "unknown format
  uint ignored" for each on every start, and a stricter client could refuse the
  schema. Map them to `integer` with `minimum: 0`.
- **`.union()` given an edge selection says "not a function".** Haiku 4.5, plate
  stand: `.union(base.edges(…))` threw a bare "not a function" at that line.
  Name what `union` takes when an argument is a selection.
- **A face's listed area misreads spline faces.** Found while making the
  part's volume and area the B-rep's (2026-10-13): `Shape_faces_json` in the
  vendored wrapper reads each face with `BRepGProp`'s fixed-order rule, which
  put a spline hook's wall 5.8 % and a six-lobe fit's 0.6 % off the adaptive
  integral. `list_entities` and the viewport's face listing quote those
  areas. Integrate each B-spline face adaptively there too, as the part's
  area now is (docs/GOTCHAS.md, "A part's volume is the B-rep's, except on
  derived surfaces"), and time it on `fitted-ring`.
- **The area integral costs a fitted ring 0.76 s.** The part's area is
  `BRepGProp`'s adaptive integral now, 4–100 ms on an ordinary part and 0.76 s
  on `fitted-ring`, whose 187-pole inset wall the volume integral already
  takes 0.78 s over (both in the mesh phase, `serve::measure`). Find where the
  time goes — the planar caps' many-pole boundaries or the walls — and whether
  the two integrals can share it.
- **A vertex treatment's `edges` is not what its `.expect` counts.** Found
  while making the edge count agree (2026-10-13):
  `box(20, 20, 20).vertices(">X and >Y and >Z").fillet(2)` reports `edges: 3`,
  the edges meeting at the corner, while `.vertices(sel).expect({ count })`
  counts corners, so pasting 3 fails with "matched 1". Report the corner count
  for a vertex target — the vertex arm of `select_edge_target` in
  `backend.rs`, through `TreatmentEdges` to `parcad_evaluation::Treatment` —
  and add a case beside `fillet-counts-a-split-rim-once`. The same arm reports
  no convex/concave split, so a corner where a seam meets an outside edge
  never carries `mixed_corners`; classify the incident edges there too.
- **The editor never shows `mixed_corners`.** The warning reaches agents in
  every reply and the window's snapshot carries it, but `app/src/state.ts`
  types a treatment without it and the treatment hover
  (`app/src/treatment-info.ts`) says nothing, so a person who writes the
  retainer's fillet by hand gets no sign it took the rim too. Show it in the
  hover beside the edge count.
- **The head-to-head, finished: 288 graded trials.** Haiku 4.5
  (`claude-haiku-4-5-20251001`, not the current Haiku), six briefs, four
  trials each: ParCAD 23/24 right and 1 wrong part reported done; text-to-cad
  8/24 and 15; kernelCAD 5/24 and 15; build123d 5/24 and 19 (p < 1e-5 each).
  On the three briefs ParCAD has no special case for (plate, lidded box,
  notch): 12/12 against 5, 3 and 1. Sonnet and Opus are right with every tool,
  partly by knowing OCCT workarounds. ParCAD ran as 0.0.11. The two ways a
  model went wrong with it — a selection spanning both kinds of corner, and
  `.expect` after a treatment — are fixed (2026-10-13, field cases
  `round-only-the-seam` and `expect-after-the-fillet`). Before quoting it:
  rerun on a release carrying them, add briefs chosen outside this repo and
  briefs that separate strong models (fit to a real object, an edit that
  changes topology, an overhang), and publish the harness.
  `research_notes/hands-on/h2h/RESULTS.md` has the tables.
- **Then Haiku 5.5, a real part, and Chinese models (same day).** Haiku 5.5,
  seven briefs × 3: ParCAD 19/21 and 0 wrong parts claimed done (the other two
  right but unvouched); build123d 17/21 and 2; text-to-cad 12/21 and 3;
  kernelCAD 12/21 and 5 — a narrower gap than 4.5's, widest on the 0 mm wall.
  The plate stand (`examples/plate-stand.js` as a brief, graded on its
  requirements) separates more: Opus is right with every tool, Sonnet loses
  kernelCAD, Haiku 4.5 fails everywhere, ParCAD included — a part in loose
  pieces that `print_check` passed, and now fails (`eval/field/does-it-hold-together.md`).
  DeepSeek v4.1 Flash, GLM 5.3 Flash and Qwen 3.8 Flash through
  OpenCode sit near the ceiling with every tool; ParCAD is the fastest arm with
  every model (DeepSeek 44 s against 134–201 s). Field runs still default to
  Haiku 4.5 (`field/run-case.sh:19`, `field/run-suite.sh:35`).

### What made modelling harder, from the head-to-head transcripts (2026-10-13)

72 ParCAD trials across Haiku, Sonnet and Opus; 27 hit at least one error or
refusal on the way (7 of those are the boss's export refusal, which is the
point). Counted by trial, most-hit first:

- **`edit_part`'s exact-text `old` missed** — 4 trials (Haiku, Sonnet), mostly
  on a second edit after the first changed spacing or quotes. Candidates: match
  ignoring whitespace runs when that match is unique; quote the nearest line in
  the refusal. kernelCAD's `set_param` (edit a named parameter, no text to
  match) is the other shape this could take for the common "make it 100 long".
- **A DSL name as a local is a parse error** — 2 trials (`const inset`).
  Parts run as `new Function(...names, source)`; wrapping the source in a block
  (`"{" + source + "\n}"`, same line numbers) lets a part's own `const` shadow a
  DSL name. That would also retire the rule that every new export is a
  compatibility change. Check QuickJS, the editor's type service and
  `editor_types.rs` agree before relying on it.
- **`fillet(selector, radius)`** — 2 trials (Sonnet), and this bench's author
  wrote it too. The refusal names the fix; taking either order when one
  argument is a number and the other a selector would remove the round trip
  without guessing.
- **The same signatures fetched every trial** — `read_docs` sections `box`,
  `cylinder`, `Shape.fillet` 15–16 times each, `sphere` 8, after the `dsl`
  index 50 times. The ten most-used signatures in the index reply would save a
  call or two per trial.
- **`wait_s` on one tool, not its sibling** — 1 trial sent it to `edit_part`.
  Small; same parameter names across the script tools.

### Kernel work for a strong-model session (Fable or Opus, high effort)

Ranked by what the bench and the competitors make urgent; each is already
described where it lives:

1. **The tag that makes the saucer crawl** (1.3 s → over 120 s, lineage
   bookkeeping), below under the planter saucer. A user hits it by adding one
   `.tag()`.
2. **`check_motion`** (queue item 5's mechanics witness): clearance certified
   over a swing on the exact solids. Limo, QymCAD and STEVE Studio all check
   motion; it is the feature gap the bench shows.
3. **The G2 blend** behind `.smooth()` / `.squircle()`, below under the
   planter saucer — prior art first.
4. **A fillet that ends exactly on a hole's rim**, in the bench section above:
   split the floor face at the touch point so the result is valid, instead of
   refusing. A user meets it on an ordinary heat sink.

### New capabilities, from the competitor research (2026-10-13)

Not fixes: things nobody in the field has, which widen what ParCAD's verdicts
are worth. Leon's read is in brackets.

1. **ParCAD as the verifier for anyone's part** [yes]. A light server, or tool,
   that takes any STEP — from text-to-cad, Fusion, SolidWorks, a person — and
   returns ParCAD's verdicts: walls, overhang, fit, between bodies, validity.
   No competitor blocks a bad part; this rides their reach instead of fighting
   it. `probe_step_export` is half of it already.
2. **Requirements first** [yes]. Before modelling, the agent turns the request
   ("80 × 50, Ø6 holes 10 mm in") into the part's own `checks`, so the verdict
   tests that it made what was asked, not only that it prints. The bench's
   commonest miss was a misread request: wrong holes, a notch on the wrong
   edge. Likely a server-instruction and `checks` vocabulary change more than
   new kernel work.
3. **A verification certificate in the 3MF** [interesting]. The verdicts and
   the script's hash in the file's metadata, shown by a slicer or on
   MakerWorld/Printables, which now require AI disclosure.
4. **A second-kernel witness.** SMLIB (usd-brep, Apache-2.0, builds here in
   18 s) rebuilds the part and the reply says whether two independent kernels
   agree on volume and validity. Its own failures on ParCAD's hard cases
   (`research_notes/usd-brep-smlib.md`) say it is a witness, not a backend.
5. **Declared relations resolved as a tree** [think later]. `mates: [{ place:
   "lid", on: "base", along: "-z", clearance: 0.5 }, …]` in any order; most
   assemblies have no loops, so each relation is one measured placement applied
   in dependency order — declarative, as models like, with no constraint
   solver. A loop (a linkage) is refused by name. Pairs with `check_motion`.
6. **Close the physical loop** [ask Klim]. After a print, a few caliper readings
   or a photo come back, ParCAD compares them with the model and tunes
   clearance for that printer.
7. **The head-to-head as a public leaderboard** [later, needs polish]. Open
   grader, briefs and transcripts; anyone submits a tool.

### Found designing ParCAD web's planter saucer (2026-09-16)

- **Which OCCT step drops a union's input.** Refused since 2026-09-17 (GOTCHAS,
  "A union that drops solids"); still unknown is where the fuse loses it, and
  whether a fuzzy value or another grouping builds it instead of refusing.
- **Delabella meshes faster and does not close.** Measured and reverted; the
  repro, the numbers and what it does not break are in GOTCHAS, "Delabella,
  OCCT's other triangulator". Worth a minimal C++ report upstream one day.
- **A tag on a union of many curved solids makes it crawl.** The same saucer
  (a dish unioned with 37 revolved B-spline bumps, 39 nodes) builds in 1.3 s
  untagged and runs past 120 s with `.tag("saucer")` on the union — the graphs
  differ only by that tag, so it is the lineage bookkeeping, not the boolean.
  The example drops the tag; the body is named by `return { saucer }` anyway.
- **`.smooth()` / `.squircle()` are declared and refused.** The DSL has had the
  curvature-continuous (G2) blend since the treatment recipes landed
  (`app/src/dsl.ts`, ARCHITECTURE "edge treatments"); the kernel rejects it until
  a true G2 surface builder exists. The saucer wanted exactly this — a bump
  easing into the floor with no crease and no jump in curvature — and got it only
  by hand-writing a smootherstep profile into a revolve. OCCT has no ready G2
  fillet; check prior art (FreeCAD, OCCT's own `BRepBlend` / `GeomFill`,
  licence fit) before building.

### Left from the bridge fix (2026-09-27)

**A filleted ceiling is never a bridge.** `overhang::held_edges` asks for
material 0.3 mm past a ceiling's edge and 0.3 mm below it. A concave round of
radius 0.3 mm or more where ceiling meets wall leaves that point in air, since
the round's face is tangent to the ceiling at the edge, and so does a wall
leaning outward past 45°. A filleted channel reads as an overhang face with no
bridge. It always did; the probe is older than the per-edge rewrite. The fix
walks from the edge across the faces that meet it until one goes down steeper
than the threshold, instead of sampling one point; decide how far a support may
be from the edge before building it.

### Left from the engine wave (2026-09-16)

In order.

1. **Threads at run time.** The pleated shade builds in 46 s in a browser tab
   against the page's 60 s, 28 s of it OCCT's mesher on one thread (3 s across
   14 cores natively). WebAssembly threads need COOP/COEP headers, which Pages
   cannot send — a service-worker shim is the known route; measure it in a real
   tab. Also check OCCT's parallel mode for booleans and fillets.
2. **The pleated shade meshes to 3.5 million triangles.** OCCT's mesher lays
   near-equilateral triangles, so the spacing a pleat tip needs is spent up the
   whole height too. An anisotropic layout needs an OCCT patch; it is what could
   bring the tab under 20 s.
3. **The pleated shade floats.** It spans z = 0.40 to 199.60; a seeded example
   stands on z = 0. Fix the example and make its case assert the z range.
4. **`thicken` refuses at 0.88 of the turn radius** (`FOLD_LIMIT`,
   `crates/parcad-occt/src/surfaces.rs`), set from four pleat depths on one
   shape, because OCCT's offset skin slows to under an eighth of the surface's
   speed there and meshes open. The geometry allows up to 1.0: fit the offset
   skin on its own parameters, as the walled loft does, and delete the limit.
5. **Errors that still don't name their cause:** 2 of 400 fuzzed fillets and
   chamfers are refused only by the generic mesh backstop.
6. **Skinned lofts the crossing check can't settle fall back to OCCT's
   self-intersection check**, which can time out on a loft as large as the
   pleated shade.
7. **Wall thickness between faces that meet is still sampled** (the diamond
   reads 35.584 against 35.551 before), outside the tool's stated guarantee.
8. **Surfaces left out:** `extend`, a patch from drawn curves, ruled surfaces,
   trimming a solid by a surface.
9. **Near a budget on a slower machine:** `walled-twisted-pleats` and
   `pleated-shade-box-cut` use ~35 % of theirs under WebAssembly here; a GitHub
   runner may be 2× slower.
10. **Small:** booleans nudge their inputs' tolerances (at most 4.7e-7 mm);
   haiku without thinking still quotes the 0.725 mm wall beside the feather in
   `where-is-the-sliver`.

## Release state, as of 0.0.6 (2026-09-15)

| channel | how it is fed | state |
|---|---|---|
| GitHub Release | tag `v*` → `release.yml` drafts every platform's files; a human reads and publishes | 0.0.6 published, latest |
| MCP Registry | `publish.yml` on publish, GitHub OIDC, no secret | 0.0.6 listed with macOS, Linux and Windows bundles |
| Homebrew tap | `publish.yml` needs `HOMEBREW_TAP_TOKEN` (fine-grained, contents on `ierehon1905/homebrew-parcad`); without it render the formula with `packaging/render.py` and push it to the tap by hand | 0.0.6 pushed by hand; `brew audit --strict` clean; upgraded and tested on the owner's machine |
| Claude Code community plugins | submitted by the owner 2026-09-14 11:11 UTC at platform.claude.com/plugins/submit (repo `ierehon1905/parcad`, path `packaging/plugin`); status only on the Console's "View submissions" page | under review: not in `anthropics/claude-plugins-community`'s `marketplace.json` as of 2026-09-15. The submission text says Apple silicon only, written before 0.0.6 |
| winget | `publish.yml` needs `WINGET_TOKEN` (classic, `public_repo`) for Komac; the first version was submitted by hand | [microsoft/winget-pkgs#435026](https://github.com/microsoft/winget-pkgs/pull/435026) from the fork `ierehon1905/winget-pkgs`: CLA signed, every validation stage passed, awaiting a moderator. Its description discloses it was AI-generated |
| ParCAD web relay | `relay/`, `bun x wrangler deploy` from the owner's Cloudflare account; the site needs `VITE_PARCAD_RELAY` at build time | deployed 2026-09-17 at <https://parcad-relay.ierehon1905.workers.dev> |

Neither secret is set, so both of those jobs skip with a notice until the owner
adds them. A release needs the build test first: dispatch `release.yml` on
`main` and tag only when all three platforms are green — 0.0.6's first test
found a clearance measured twice that timed the 20-turn thread case out on every
runner.

## How a queue item is run

Each item above ran as one background agent session in its own git worktree,
then was reviewed here before it reached `main`. What made that work, and what
a new session should keep doing:

- **The brief** names the item in this file, the docs to read first, and asks
  for *prior art evaluated and why chosen or rejected* before any design
  (OCCT samples, CadQuery/build123d, FreeCAD, established tools), with licence
  fit: MIT/Apache crates; Apache code portable with attribution; LGPL read
  only, and every change to `vendor/` recorded in its PARCAD-CHANGES.md; MPL
  file-level.
- **Safety rules in every brief**: start a host with scratch
  `PARCAD_PROJECTS_DIR` and `PARCAD_SEED_DIR` before any `parcad call` or
  `parcad tools` — a bare `parcad` command hosts against the real project folder and
  seeds parts into it, which one session did; never push, never touch `main`,
  never force-push, no repository settings; commit with `TZ=UTC`; wait for long
  commands to finish (a session once stopped "waiting" on a background gate that
  was gone); build native with `PARCAD_OCCT_PREBUILT` pointed at an existing
  `target/release/build/occt-sys-*/out`.
- **Evidence asked for**: closed forms with independent derivations in
  `eval/cases/`, the corpus unchanged except where a case says why, a field
  case run for anything a model reads, and renders of what was built.
- **Review before merge**: re-derive the recorded numbers by hand; diff every
  existing case's expectations against `main`; check the project folder and
  its `.seeded` were not touched; rebase on `origin/main` and run `tools/check.sh`;
  try the feature on a part the session did not write; look at the renders; then
  fast-forward `main` through the pre-push gate. Remove the worktree afterwards —
  each holds 5–9 GB of `target/`.
- **Outward-facing actions** (a pull request, fork, comment or submission outside
  the owner's repositories) get an explicit yes for that action first, and say
  up front that they were AI-generated.

---

## 1. Shipping — what still stops a second machine

- **Signing.** The `.app` is unsigned and un-notarised, so a second Mac meets
  Gatekeeper before it meets the kernel; nothing is signed on any platform.
  See "Waiting on a decision".
- **The updater has not delivered a real release yet.** The app checks
  GitHub Releases and installs signed updates (packaging/README.md, "The
  updater"). 0.0.6 and earlier have no updater, so their users reinstall by
  hand once. It was tested locally from a fake older build. Homebrew's
  `parcad` is updated by `brew upgrade`, not by this.
- **The Linux and Windows bundles have not been opened on a desktop**, and the
  Homebrew formula is macOS only. Linux arm64 measures 109 of 110 (the fillet
  bisection in `refuse-unblendable-junction` lands on 1.25 mm) and is not a
  release target.
- **Linux x86_64 refused the `untriangle-v3` seed part** *(fixed)*. Two of
  its mesh edges came out open there and nowhere else — arm64 under GCC and
  Clang agree to the triangle. Not the compiler: BRepMesh left each twisted
  wall's straight ruling as one boundary link however far the normal turned
  along it, and the interior chased that turn in a chain of nodes closing
  on the ruling that only the pass cap ended — on x86_64 at 0.0008 mm from
  the ruling, inside parcad's 0.001 mm weld, which fused the two walls' last
  nodes into one vertex; arm64's rounding stopped a pass short. Vendor patch
  0005 splits a boundary segment where the normal turns more than an
  interior link may, a node both faces share; docs/GOTCHAS.md has the
  mechanism.
- **`pipe-tee` is refused in an emulated x86_64 container, and builds on
  real x86_64.** Under GCC 13 on Ubuntu 24.04, run as linux/amd64 in Docker
  on Apple silicon, the self-crossing check reads its 2 mm blend as a face
  crossing itself near (22.636, 0.000, 24.068) and refuses it. The v0.0.7
  release run (35214816552) built it on a real ubuntu-22.04 x86_64 runner
  under GCC 11 to the same numbers as macOS, 134874.46 mm³ and 12824
  triangles, and Windows x86_64 passed too. The container differs in
  compiler, libc and CPU emulation at once, so which one moves the exact
  intersector is not isolated; a container reproduces CI's result only where
  the two agree, as `untriangle-v3`'s did.
- **A stale sidecar ships silently.** `tauri build` does not run
  `tools/build-worker.sh`: a missing staging copy fails loudly, a stale one
  bundles last week's kernel and measures parts confidently with it. Free to
  close.

## 2. Depth over breadth, and the shape question behind it

**Measured rather than assumed.** Twenty-one of the reference corpus's Fusion
documents were exported and counted. Thirteen have surfaces this kernel cannot
make; fifteen are more than one solid; Loft and Sweep each appear in six designs,
more than Revolve, which we did build. The counts and their caveats are in
[DSL_GAPS.md](DSL_GAPS.md), "What twenty-one real Fusion 360 designs actually
needed" — evidence about priorities, not a specification.

Two honest readings, which are different products:

- **Narrow.** Machined metal: brackets, fittings, manifolds, flat faces and round
  holes. parcad is already unusually good here, the missing shapes barely matter,
  and "the only CAD an agent can use *and* verify" is a stronger position than a
  weaker Fusion. Recommended.
- **Wide.** Flowing surfaces and multiple solids. Long, serious, and a direct
  fight with tools that have had twenty years.

**This is a product call, not a technical one, and it is not made yet.** Nothing
below this section should be started until it is: the two readings disagree about what
is worth building next, so building either first is a bet placed early.

## After the laptop holder — what one part's worth of friction left open

On 2026-09-12 a model built a VESA-mounted V holder for a 16" MacBook Pro
and the day's cost went into [DSL_GAPS.md](DSL_GAPS.md) §9 and
[SELECTORS.md](SELECTORS.md). Most of it was fixed the same day: selectors
by angle, feature and length, four kernel defects, a table of real objects,
a fit check, lines and hulls in the plane, and three field cases that a
small model reads SOUND. What is left, in the order it would pay:

1. **A sketch the model can see.** The one input that moved the design was
   a drawing over the render, done on a phone through a throwaway page. In
   the app it is a canvas over the viewport, a PNG beside `part.js`, and one
   MCP call that returns it. Half a day.
2. **The rest of the selector proposal**: `faces()` as a query, `any`, and
   the compact string lowering to the object form so there is one spec.
   SELECTORS.md §3 lists them. A day.
3. **Construction geometry in the graph.** `line2d` and `hull` are
   arithmetic on the script's own numbers; a line taken *from a built edge*
   needs the graph to carry it, which is OP_ROADMAP's construction-plane row
   and the real "place against geometry". A week, and the product call in
   §2 above applies.
## Text on a part — asked for, not started

Wanted for labels, legends and model codes on instrument-like parts: small,
lowercase, engraved or raised a fraction of a millimetre. Nothing in the
language makes a glyph today. Since item 4 (curves in sections) most of one
does: a TrueType outline is quadratic Bézier contours, which an `extrude`
section already builds exactly and re-entrant, and a counter (the hole in `o`)
is a second extrude cut from the first. What is missing is the font.

**Prior art.** Fusion splits it in two: Sketch › Text makes a profile, and
Solid › Emboss projects it onto a face as *emboss*, *deboss* or *scribe*, with a
depth, on developable faces only. CadQuery's `text()` and build123d's `Text` go
through OCCT's `Font_BRepTextBuilder`; OpenSCAD has `text()` plus
`linear_extrude`. The OCCT route needs FreeType and system fonts, and
`vendor/occt-sys/build.rs` builds with `USE_FREETYPE` off — a C dependency and a
part that measures differently on another machine.

**The likely shape: no kernel change.** One OFL font, so every machine — and
ParCAD web — builds the same outline. A build step reads it
(`ttf-parser`, MIT/Apache, or a script) into a generated glyph table of contours
and advance widths, the way `docs.rs` is generated from `dsl.ts` and never
written beside it; the DSL function lays out a string from that table into
`extrude` sections with `{ bezier }` entries, unions the outer contours and cuts
the counters. Nothing reads a font at run time, so the QuickJS sandbox needs no
file access. A flat solid on a plane first; wrapping onto a cylinder is
Fusion's harder half and waits. Eval cases hold it to closed forms: a glyph
with a counter against its contour areas (shoelace plus the Bézier segment
areas) times depth, and a word's bounding width against the table's advances.

**Open decisions.**
- *The name.* Every export is reserved in a script, and `text` is a likely local.
  Fusion's word for the solid-making half is `emboss`.
- *The font.* One typeface for every part; a comparison sheet is the input.
- *Minimum stroke.* A light weight at 2 mm cap height is under a 0.4 mm nozzle;
  the op should refuse or report the thinnest stroke, measured, like
  `measure_wall_thickness`.

## Integrations before launch — researched 2026-09-16

Two web research passes, one on the 3D-printing side and one on CAD and the
web, ranked what to connect ParCAD to before launch. 3MF export and
`export_part`'s `open: true` are built; left of them: a 3MF has not been opened
in a real slicer, the field case `one-file-for-the-slicer` has not been run,
colour per body (`basematerials`) waits for materials to reach export, and
`open: true` has only been measured failing (no slicer on this machine).

### STEP with body names and colours — next, its own session

`vendor/opencascade`'s `write_step` uses a plain `STEPControl_Writer`, so a
part's bodies arrive in other CAD programs unnamed and grey. The XCAF toolkits
that carry names, colours and assemblies (`TKXCAF`, `TKDESTEP` via
`STEPCAFControl_Writer`) are already linked in `vendor/opencascade-sys/build.rs`
— it is a binding change, recorded in `PARCAD-CHANGES.md`, with no new
dependency.

- **Names:** each `Op::Bodies` entry's name on its solid; for a one-solid part,
  the project name.
- **Colours:** from `.material()` once the `materials` worktree lands.
- **Units:** write millimetres explicitly. The classic STEP failure is a
  25.4× or 1000× scale from a misread header unit, so a corpus case re-reads
  each export with `probe_step` and holds its volume and bounds to the build.
- **What importers show (unverified here):** FreeCAD reads colours through
  OCCT, Fusion keeps simple RGB, Onshape reportedly stopped importing STEP
  colours in 2024, and whether Fusion keeps body names is unconfirmed. So the
  docs say "where the importer supports it". Estimate 2–3 days.

### The in-chat viewer — built, not yet seen in a real client

`open_project` shows its part in 3D in MCP Apps clients (ARCHITECTURE, "The
part inside a chat"); it was checked only in the reference host from
`modelcontextprotocol/ext-apps`. Left: open it in Claude Desktop and web, where
the iframe and message size limits are the client's; a GLB export, which
`<model-viewer>` embeds and iOS AR Quick Look would use.

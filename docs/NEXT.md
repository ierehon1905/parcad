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

1. **Checks that run without being asked** — agreed 2026-09-16, next. Two
   parts shipped as STLs from one session with defects `measure_wall_thickness`
   finds at once: a 0.013 mm sliver between a cable channel and a slot, and a
   grille cutting 0.319 mm into a screw boss. The model (Opus, with a memory
   note saying to check) never called it, and every number `export_part`
   returned passed. A check a model must remember is a check weaker models
   skip, so it moves onto the route every model already takes. The report is
   already trustworthy (thin readings classified and grouped into places, every
   feather and face pair found; `where-is-the-sliver`); what is left:
   - **Collisions**: for each cut, the named features it removed material from
     besides its target — "grille cuts boss" needs no threshold.
   - **On the route**: `export_part` and `save_project` carry a `print_check`
     (thinnest wall, its two features, collisions) from the cached build, its
     verdict the first line of the reply. Below a floor nothing prints (≈0.3
     mm) export refuses, naming the spot and the fix, unless given
     `allow_thin: "reason"`; between that and the process minimum it flags.
   - **Measured**: an `eval/field/` case whose part hides a sliver and a
     collision; SOUND only when the reply tells the user both, both arms.
   - Server instructions and the skills say a part is done when `print_check` is
     clean or each flag has a reason — the weakest layer, and one line.
2. **Two more steps a model has to remember, found the same day.** Both are
   the shape of item 1: the mechanism exists, nothing makes it happen.
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
3. **Homebrew is published but never exercised.** `publish.yml` renders the
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
4. **WebMCP on the playground** — agreed 2026-09-15, waiting on the owner's
   origin-trial token; the playground is live at
   <https://ierehon1905.github.io/parcad/>. Time-boxed, page-only. The
   playground's one missing surface is MCP;
   [WebMCP](https://developer.chrome.com/docs/ai/webmcp) lets the page register tools for an agent in the visitor's browser (Chrome 149+, behind
   `chrome://flags/#enable-webmcp-testing` or an origin trial; consumed by
   Chrome's Gemini agent and the Model Context Tool Inspector extension, not by
   Claude or Cursor). The tools are the ones `mcp.rs` already serves — same names,
   descriptions generated from it at build time, never a second copy — each
   calling the same `backend.ts` function the editor calls, so an agent's part
   appears in the visitor's editor. Out of reach in a tab: the shared live
   session, parts as files, builds over 60 s. `field/` cannot grade it (it speaks
   MCP, and this is not an MCP server), so the evidence is inspector-extension
   trials and a written record of whether an agent finds the tools, reads the
   replies, and builds a part the editor shows; a weak result is recorded and not
   shipped. Needs from the owner: an origin-trial token for
   `ierehon1905.github.io`. Check on the live site that the document is
   origin-isolated, which WebMCP requires.
5. **A measured parts library** — fasteners, bearings, boards, devices, each
   held by eval cases, and a way for one part to import another.

### Waiting on a decision

- **How the playground is published.** Pages serves the `gh-pages` branch;
  `playground.yml` is disabled (manually, 2026-09-15). The recipe used since:
  `playground/build-kernel.sh`, the corpus against `playground/node-worker.sh`
  (all green or no deploy), `playground/prebuild.sh`, `vite build --mode
  playground` with `PARCAD_PLAYGROUND_BASE=/parcad/`, then the output committed
  to `gh-pages` as "Playground from main <sha>" and pushed with the repository's
  pre-push hook off (it runs the code gate, which a branch of build output
  cannot). Either script that recipe or re-enable the workflow; it is written
  down nowhere else.
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
- **Signing**: SignPath Foundation (free) for Windows; Apple Developer ID ($99/yr)
  for the `.app`. Neither is started.

### Found designing the playground's planter saucer (2026-09-16)

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
  `parcad tools` — a bare `parcad` command hosts against `~/Documents/parcad` and
  seeds parts into it, which one session did; never push, never touch `main`,
  never force-push, no repository settings; commit with `TZ=UTC`; wait for long
  commands to finish (a session once stopped "waiting" on a background gate that
  was gone); build native with `PARCAD_OCCT_PREBUILT` pointed at an existing
  `target/release/build/occt-sys-*/out`.
- **Evidence asked for**: closed forms with independent derivations in
  `eval/cases/`, the corpus unchanged except where a case says why, a field
  case run for anything a model reads, and renders of what was built.
- **Review before merge**: re-derive the recorded numbers by hand; diff every
  existing case's expectations against `main`; check `~/Documents/parcad` and
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
- **Never run from a fresh user account** — neither the `.app` nor
  `brew services`. Whether a launchd agent gets `~/Documents` without a TCC
  prompt there is the one claim the README does not yet make;
  `PARCAD_PROJECTS_DIR` is the way out if it refuses.
- **The Linux and Windows bundles have not been opened on a desktop**, and the
  Homebrew formula is macOS only. Linux arm64 measures 109 of 110 (the fillet
  bisection in `refuse-unblendable-junction` lands on 1.25 mm) and is not a
  release target.
- **Linux x86_64 refuses the `untriangle-v3` seed part.** GCC's build meshes
  two of its edges open, so the watertight backstop refuses it; macOS builds
  it. `eval/cases/untriangle-v3.json` marks it `known_defect_on` that platform.
  It passed there at 0.0.6, so something since changed the mesher's result.
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
4. **Measure the radii.** `DEVICES` carries corner and edge radii read off
   photographs, labelled so. A caliper on each machine settles them for
   everyone. An hour with the hardware.
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
the WebAssembly playground — builds the same outline. A build step reads it
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

`evaluate_part` shows its part in 3D in MCP Apps clients (ARCHITECTURE, "The
part inside a chat"); it was checked only in the reference host from
`modelcontextprotocol/ext-apps`. Left: open it in Claude Desktop and web, where
the iframe and message size limits are the client's; a GLB export, which
`<model-viewer>` embeds and iOS AR Quick Look would use.

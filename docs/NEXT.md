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

## The queue, as of 2026-09-14

One session at a time, because each rewrites the same core files (`graph.rs`,
`backend.rs`, `dsl.ts`):

1. **Parts with more than one solid** — done 2026-09-14. `return { base, lid }`,
   measured per body and between bodies, one STEP solid per body; probes, tag
   extents and wall thickness run per body since item 2.
2. **One engine** — done 2026-09-15. The exact kernel is the only one; the
   implicit backend (`sdf.rs`, fidget) is deleted. It had refused 33 of the 41
   parts in a real project folder, read `blend` as a different shape, and
   measured probes and wall thickness with every fillet dropped. What replaced
   it, in `crates/parcad-occt/src/perceive.rs`: point classification and
   distance (`BRepClass3d_SolidClassifier`, `BRepExtrema` against the boundary
   faces), rays as intersections with every face met (`BRepIntCurveSurface_Inter`,
   transition folded through face orientation), a thickness sweep over the
   tessellation's nodes and a barycentric grid on every triangle, tag extents
   from face lineage, and renders and region maps off the tessellation with a
   pixel-per-face map. Five corpus cases hold closed forms — a Ø12 bore in a 40
   mm plate reads 14 and 28 mm along X and a point 100 mm up reads √(97²+6²) =
   97.185 to the bore's rim; a wall under an r = 2 fillet reads 4 + 2 + √3 =
   7.732 where the field read 8 as an upper bound. Speed: the worker is
   long-lived (`Frame`/`@reply` on stdin/stderr, a pool of two, crash isolation
   kept — a dead worker is replaced, a hung one killed) and keeps a per-subtree
   `BuildCache`, so a second identical build of `plate-stand` is 475 ms wall
   against 3223 cold and a root-level edit 697 (212 in the kernel); the corpus
   numbers did not move. The window has no kernel toggle; `--brep` and
   `--depth` say they do nothing. The field suite read every perception case
   SOUND before the deletion: does-the-port-meet 8/8, does-the-laptop-fit 8/8,
   which-backend-measured 8/8, how-thin-is-it 8/8, what-is-hidden 8/8,
   where-is-the-feature 8/8, what-is-inside 6/8 SOUND and 2/8 LUCKY.
3. **Threads that close** — done 2026-09-15. The helical cut that "opened at
   three turns" was parcad's own seam-pcurve pass, not the boolean, and was
   already fixed on main by a guard added for a different part; the helix
   commit's worker opens by the recorded counts and closes with only that
   guard applied (GOTCHAS, "A helix cut through its own cylinder").
   `threadedRod` and `threadedHole` ship as `Op::Thread`, the ISO 68-1 basic
   profile swept along a helix of one edge per turn — FreeCAD's construction,
   chosen over cq_warehouse's ruled faces, bd_warehouse's lofted loops and
   OCCT's MakeBottle `ThruSections` by measurement — and every build is held to
   the slab closed form at 2e-5. Two finishes that looked equivalent returned
   valid closed solids 16–100% short (GOTCHAS, "Threads"). Cases:
   `thread-m8-{3,8,20}-turns` (both hands), `thread-bolt-and-nut` (0.2 mm
   clearance read back as 0.200), `helical-groove`, `refuse-thread-clearance`,
   and the seeded `screw-top-jar`. The field case `does-the-bolt-turn` (a
   printed M6 bolt and nut from nothing but `read_docs`) read 3/4 SOUND and
   1 LUCKY at thinking 8000 and 2 SOUND, 1 LUCKY, 1 WRONG at thinking 0; every
   LUCKY is the scorer matching "the script" in a reply that quoted
   `between_bodies`, and the WRONG trial centred nothing where the docs say a
   rod is centred and unioned a shank through the nut. A first round, before
   the docs showed a mating pair, lost a trial to moving an out-of-phase nut
   off the thread; a second lost one to reading "interfering" as phase when
   the hole stopped short, which is why `threadedRod` now names both.
3a. **Three render faults several bodies exposed** — done 2026-09-15.
   - *A tagged copy painted as its original.* `split-halves` drew all
     952395 iso pixels `left`, `right` 0: every face of
     `left.mirror("x").tag("right")` carries both names, and a pixel took the
     first in node order. A tag on a move, turn, scale or mirror now outranks
     the tags inside its input, on the copy only (`perceive::face_tags`,
     `NamedFaces::outranked_by`); an untagged `.at()` names nothing, so a
     feature tag before a placement still wins. After: 399447 `left` and
     552948 `right` (0.419 / 0.581, against 0.407 derived from projected
     areas); front and top views read 0.5000 each, pixel-exact reflections.
     Of 97 corpus and seed scripts only `split-halves` writes a tag over a
     tagged input, so no recorded surface or thickness moved.
   - *An overlap of two bodies drawn as a hole.* One crossing parity over two
     closed shells is even where they overlap. The raster keeps one parity bit
     per body from the worker's body spans, cut face where any body is odd.
     `interfering-bodies` cut on Y: 501 mm² of cut face before, 551.8 front
     and 549.6 iso after, against 400 + 200 − 50 = 550; iso cut fraction
     0.3571 → 0.3926.
   - *A dark line down an M10 bolt's cut face.* Not per-body, not the
     tessellation: 138 uncapped samples all on the iso buffer's centre column,
     where the nut's corner edge projects exactly and a sample on a shared
     edge failed both triangles' f32 test, losing a crossing; 16 more on
     thread flanks from the clip reading the rounded depth. Fixed-point corners
     with the top-left fill rule, and the unrounded depth (GOTCHAS, "A section
     cap with a line through it").
   Cases: `renders` blocks in `split-halves` and `interfering-bodies`, the new
   `section-m10-bolt-and-nut`, each failing on the old rasteriser or worker
   (right 0 px; 501.3 / 499.7 mm² and 2201 / 1249 overlap pixels open; 138
   rod pixels open); unit tests for the per-body cap and for the fill rule on
   a rod at four sizes that drew the line. Field, against the new host:
   what-is-hidden 8/8 SOUND; what-is-inside 7/8 SOUND, 1 WRONG — a trial that
   read its x = −22 section as two separate circles, on a picture measured
   pixel-identical before and after the change.
4. **Curves in sections and paths** — done 2026-09-15. One section type for
   extrude, revolve, loft and sweep: corners, rounded corners, through and
   radius arcs, spline, Bézier and B-spline curves, a loft that ends on a
   point, a spline sweep path — resolved to exact arcs and B-spline poles in
   `section.rs`, never polygonised, with polygons on their old path (94 corpus
   cases re-recorded unchanged). Re-entrant sections build; a crossed polygon
   is refused by name, a crossed curve by BRepCheck. Twelve closed-form cases
   (OP_ROADMAP §2) and four refusals; `curve: "spline"` selects curve edges.
   Of the four Fusion targets said to wait on it, one moved: `untitled2-v1`,
   both bodies from the export's degree-5 poles, +0.008% and +0.0004% on
   Fusion's volumes. `v2`'s sections turned out to be a square, circle, turned
   square and a point — they build, but OCCT's smooth fit is 2% short in area;
   `v3` is an ornament of cones, `v4` a fitted curve under a mismatched header.
   `hydraulic-line` draws its gland as the catalogue section. The field case
   `draw-the-round-ends` (a gasket with 5 mm corner radii and a round-ended
   slot, 14 faces and 2000 mm³ only with true arcs) read 1/8 SOUND on the
   first docs — every trial found `SectionEntry`, and the failures were
   `{ radius }` bulging inward on a clockwise listing, `{ at }` read as the
   arc's start, a hole listed as a second loop, and a "30 mm overall" slot
   drawn 40 long. `radius` now bulges out whichever way the corners run, and
   the docs say what `at` is, that a section has no holes, and where a slot's
   corners sit; the second round read 8/8 SOUND (4/4 in each thinking arm),
   every trial drawing arcs rather than filleting a box. The slot arithmetic
   in the docs is close to what this case asks, so the second round measures
   the docs being read more than the vocabulary being inferred.
5. **A playground in the browser** — built 2026-09-15, not yet published. The
   vendored, patched OpenCASCADE and parcad's own Rust compile together with
   Emscripten 6.0.9 for `wasm32-unknown-emscripten` (`playground/build-kernel.sh`,
   native Wasm exceptions, single-threaded): the worker under Node, which the
   corpus drives unchanged and passes 113 of 113 (67 s; native 27 s), and
   `crates/parcad-wasm`, which runs `parcad_occt::serve` and
   `parcad_evaluation` — moved out of the worker binary and `service.rs` so
   there is one definition of each — in a Web Worker behind a third transport in
   `backend.ts`, with parts in IndexedDB. Recording the corpus under both builds,
   topology and every refusal are identical and only tessellation moves: at most
   1.7e-4 in volume or area. `re-entrant-loft` holds its triangle count to 260%,
   traced to one Delaunay insertion of a point on the link it splits (GOTCHAS,
   "A planar wall meshes two ways"). The kernel is 19.7 MB, 6.4 MB gzipped, 4.3
   MB brotli; the bracket's snapshot in headless Chrome equals the native host's
   field for field, first part on screen 650 ms after navigation locally. Pages is
   enabled (source: GitHub Actions) and `.github/workflows/playground.yml` is
   dispatched by hand; headless Chrome and
   headless Firefox build the bracket from a plain static server, Safari is
   untested, and on a phone it has not been opened. playground/README.md has the
   recipe, the table and what a visitor can and cannot do.
5a. **WebMCP on the playground** — agreed 2026-09-15, after the first Pages
   deploy is live; time-boxed, page-only. The playground's one missing surface
   is MCP; [WebMCP](https://developer.chrome.com/docs/ai/webmcp) lets the page
   register tools for an agent in the visitor's browser (Chrome 149+, behind
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
5b. **Checks that run without being asked** — agreed 2026-09-16, next. Two
   parts shipped as STLs from one session with defects `measure_wall_thickness`
   finds at once: a 0.013 mm sliver between a cable channel and a slot, and a
   grille cutting 0.319 mm into a screw boss. The model (Opus, with a memory
   note saying to check) never called it, and every number `export_part`
   returned passed. A check a model must remember is a check weaker models
   skip, so it moves onto the route every model already takes:
   - **Make the report trustworthy first.** The old part flagged 1072 samples
     at 1.2 mm, nearly all intended (1 mm pocket floors), with the real defect
     one unnamed line among them. Cluster samples into places ranked by
     thickness; tell a sharp edge (a 75° slot lip read 0.319) from material
     between two features; name untagged features by the node and script line
     that made them.
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
6. **A measured parts library** — fasteners, bearings, boards, devices, each
   held by eval cases, and a way for one part to import another.

### Waiting on a decision

- **Publish the playground**: enable GitHub Pages for the repository and run
  the deploy, once item 5 is reviewed and merged.
- **A tag inside a tagged copy.** A tag on a move or mirror now outranks the tags
  inside what it copied (item 3a). The session also let the copy's name beat a
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

## 1. Shipping — the bundle carries its kernel; nothing else about shipping works

**Done: the sidecar.** `parcad-occt-worker` is an `externalBin`,
`tools/build-worker.sh` stages the triple-suffixed copy the bundler demands, and
[`worker_path()`](../crates/parcad-occt/src/host.rs) accepts the bare or the
suffixed name — Tauri 2.11 on macOS strips the suffix again, recorded there as a
measured fact rather than a contract. Verified the only way it counts:
`ParCAD.app` installed to `/Applications`, launched with `PARCAD_OCCT_WORKER`
unset, evaluating `examples/bracket.js` over the HTTP host at **55074.791 mm³,
80 × 60 × 44 mm, watertight, 22 faces / 107 edges** — the value
`eval/cases/bracket.json` records. With the worker deleted from the same bundle
the evaluation refuses and names the rebuild.

**Still not shipping, and the next step costs money rather than time.** The
bundle is unsigned and un-notarised, so a second machine meets Gatekeeper before
it meets the kernel; signing needs an Apple Developer Program membership, which
is a decision, not a task. There is no updater. The `.app` has been run from
outside the build tree, never from a fresh user account.

**Done 2026-09-14: Linux and Windows build, measure and bundle.** `release.yml`
runs macOS arm64, Linux x86_64 (Ubuntu 22.04, for glibc reach) and Windows
x86_64 (MSVC), and on each one measures all 110 corpus cases with the worker it
uploads. Windows needed `taskkill` for a wedged worker, NTSTATUS crash codes,
the `.exe` names, C++17 asked for through `cc` rather than a flag MSVC ignores,
LF checkouts for the patch series, and OCCT's `libd/` install directory. Linux
x86_64 moved three mesh-derived numbers, now held to tolerances their cases
explain. Linux arm64 builds in Docker and measures 109 of 110: the fillet
bisection in `refuse-unblendable-junction` lands on 1.25 mm rather than 1.13,
because a 1.25 mm blend builds under that compiler. It is not a release target
yet, so the case still records 1.13. Not yet done: the Linux and Windows
bundles have not been opened on a desktop, the Homebrew formula is macOS only,
and nothing is signed on any platform.

**One hazard is free to close and worth closing.** `tauri build` has no
dependency on `tools/build-worker.sh`. A *missing* staging copy fails the build
loudly; a *stale* one silently bundles last week's kernel and measures parts
confidently with it — the exact shape of wrongness this project refuses
everywhere else, currently held by a sentence in a document rather than by the
build.

**Done the same day, except the measurement: Homebrew installs a tool, not
a window.** Decided 2026-09-14. The
cask ships the `.app`, and a cask is the wrong container for what Homebrew
users of this actually want: the MCP server up at login and the UI on
<http://127.0.0.1:4242>, which the browser already gets whole. A cask is also
the container that gets quarantined, so the `xattr` line in the README is a
cost of the packaging, not of the code. The plan, in order:

1. **A host crate with no Tauri in it.** `service`, `projects`, `script`,
   `session`, `docs`, `http` and `mcp` move from `app/src-tauri` to
   `crates/parcad-host`; the desktop app keeps only the IPC adapter and the
   window. The HTTP host takes its frontend from an `Assets` provider — the
   Tauri resolver in the app, an embedded copy of `app/dist` in the CLI — so
   there is still exactly one frontend build.
2. **`parcad serve`** in the CLI: seed the project folder, bind the port,
   print the URLs, run until stopped. Same router, same MCP, same session.
3. **A formula in place of the cask**, pointed at the
   `parcad-cli-<arch>-apple-darwin.tar.gz` the release workflow already
   stages: `parcad` and the worker in `libexec`, a `bin/parcad` env script,
   a `service` block for `brew services start parcad`. Bottles are not
   needed; a prebuilt-binary formula is the ordinary shape for a tap.
4. **Measure, do not assume, the project folder from a launchd agent.**
   `~/Documents` is behind a TCC prompt for a bundle; whether a bare binary
   started by `brew services` gets the prompt, a silent refusal, or the
   folder is a fact about macOS to establish on a clean account before the
   README promises it. `PARCAD_PROJECTS_DIR` is the way out if it refuses.

The zip on Releases stays for whoever wants a dock icon. What this does not
do: sign anything, or move the product call in §3.

Items 1–3 landed: `crates/parcad-host`, `parcad serve`, and
`packaging/homebrew/Formula/parcad.rb`, which points at 0.0.3 and needs its
sha256 filled in when that release exists. Beside them, because the CLI now
embeds the host: **`parcad tools` and `parcad call`**, an MCP client of the
running host, so every tool a model can call a shell can call with the same
arguments and the same reply — parity by construction, not by a second list
— and a `.js` script is accepted wherever the CLI took a graph. The stdio
shim the "Also true" section below asks for is now that client plus a loop
over stdin; hours became one.

Item 4, measured on 2026-09-14 on the author's machine: the formula
installed from a local tap, `brew services start parcad` brought the host
up under launchd, and `parcad call list_projects` through it listed
`~/Documents/parcad` and `evaluate_part` built the bracket at 55074.791 mm³
— no TCC prompt, no refusal, nothing quarantined (`xattr` shows only
`com.apple.provenance`). Still owed: the same on a fresh user account, where
Terminal has never been granted Documents access. That is the one claim the
README does not yet make.

## 2. One live session the agent can drive — built

`session.rs` beside `service.rs`; `open_project` / `set_script` / `get_session`
over MCP; viewers pushing their document on the existing debounce, and ignoring
what they originated. An agent edit is an ordinary edit that Cmd-Z takes back;
there is no lock. The account is in [ROADMAP.md](ROADMAP.md) under "Built: one
live session the agent can drive"; whether a model drives it is measured by
`eval/field/change-the-open-part.md`, one trial at a time. The prerequisite this
item named — one `EvaluationSnapshot` every transport serialises — landed first
and holds.

Worth keeping, because this file was wrong about it: the duplicate definition of
an evaluation was never in `mcp.rs`, which had been fixed in `9cbdd443`. It was
on the far side of the UI transport, in `main.ts`'s own `Report` interface (now
`state.ts` and `engine.ts`). Both roadmaps can run a commit or two behind the
code — check before believing either, including this one.

## 3. Depth over breadth, and the shape question behind it

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
below item 3 should be started until it is: the two readings disagree about what
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
   §3 above applies.
4. **Measure the radii.** `DEVICES` carries corner and edge radii read off
   photographs, labelled so. A caliper on each machine settles them for
   everyone. An hour with the hardware.
5. **Two solids that stay two — done, from the cheap end.** `return { base,
   lid }` builds a root-only `Op::Bodies`; the worker builds, meshes and
   checks each body on its own, the reply measures each (`named_bodies`, with
   a per-body `pieces` count for the accidental split) and every pair on the
   exact solids (`between_bodies`, the `check_fit` measurement), STEP writes a
   solid per body, and `export_part` takes a `body`. Five corpus cases hold it
   to closed forms — two boxes, the two printable halves, an interfering
   pair, a body split inside itself, and the seeded `lidded-box`; the field
   case `does-the-lid-clear` asks a model the question and has not yet been
   run. Joints and mates stay out; the §3 call is not moved.

## After the unicorn — two outside models' reports, checked against the code

On 2026-09-14 a Codex session built a figurine over MCP (`selene-unicorn`,
125 × 50 × 141 mm, 7½ minutes). It and a second model wrote up what they would
change. Each claim was checked against the code before it went here, because
the last outside report called two shipped ops impossible. The reports ranked
trust in the result first, then the window, then speed, then organic shapes.
Measured, then fixed the same day. What each became:

1. **"Unions lost the body": true, and it was the mesh. Fixed.** The B-rep was
   exact, and its tessellation was a watertight fragment that every number, the
   preview and the STL read. Three representation defects left by a union with
   a rotated copy of itself are now repaired after every unify. The worker
   compares mesh volume with solid volume, and refuses a face the mesher
   skipped, as backstops. [GOTCHAS.md](GOTCHAS.md), "A correct solid can mesh
   as a closed fragment of itself".
2. **The window kept the old model: true, four causes, all closed.** Windows
   report the revision they evaluated, whether it `built`, and its volume.
   `set_script` and `open_project` wait for that report and return it as
   `viewers`. The event stream sends the current state on connect. A reloaded
   window follows the session. A push based on a replaced revision is refused.
   Setting the same script again re-evaluates everywhere. Measured by
   `eval/field/did-the-window-draw-it.md`: 6/6 SOUND, both arms.
3. **Renders were not files: fixed.** Every view `evaluate_part` draws is also
   a PNG with its `path` in the reply. `save_project` writes `preview.png` and
   says whether the script built.
4. **Nothing was cached: fixed.** The last eight exact builds are kept by
   graph, so evaluate → render → export → window is one kernel run
   (`reused_build`). `evaluate_part` and `export_part` take `timeout_s`, and the
   timeout refusal names it. **No job ids or progress.** With the cache and a
   caller's budget, nothing measured yet needs them.
5. **Errors lacked a place: fixed.** A script error names its line and quotes
   it, in the sandbox and the window. `node N (label)` in a kernel refusal reads
   `node N (line L, label)`. MCP errors carry `data`: `kind`, `line`, `node`,
   `stage`.
6. **`export_part` omitted quality: fixed.** It returns `measured`: size, volume,
   `watertight`, `bodies`, `voids`, and the STL's `deflection_mm`.
7. **No snapshots: fixed.** `save_project` keeps the `part.js` it replaces, and
   `set_script` keeps the on-screen text, saved or not, under
   `.history/<part>/`. `list_snapshots` and `restore_snapshot` bring either
   back, 50 per part.
8. **Organic shapes.** `scale(x, y, z)` builds an exact ellipsoid through
   `BRepBuilderAPI_GTransform`, held to the determinant (`eval/cases/ellipsoid`).
   It also exposed `BRepGProp`'s fixed-order integral misreading B-spline
   faces. `pipe` and `sweep` take a `{ helix }` path (with `endRadius`, a
   horn) and a `taper`, B-rep only, each held to a closed form
   ([OP_ROADMAP.md](OP_ROADMAP.md) §5). Threads are
   `threadedRod` and `threadedHole`, held to closed forms (queue item 3).

Already true and misreported: `preview_ready` / `exact_ready` do not apply,
because the window runs one exact kernel per evaluation and the snapshot names
it in `backend`.

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
web, ranked what to connect ParCAD to before launch. What was built, and the
two that wait for their own session:

- **3MF export — built 2026-09-16.** One named object per body, millimetres,
  core spec only (`parcad_core::threemf`, a hand-written deflated ZIP). On
  `export_part` as `format: "3mf"`, in the export menu, on `/api/export/3mf`
  and in the playground. lib3mf 2.5.0 (the 3MF Consortium's reference reader)
  opened `lidded-box`, `screw-top-jar` and `knurled-knob` in strict mode with
  no warnings, every object manifold and oriented, each object's volume equal
  to `named_bodies`. Not yet opened in a real slicer. Colour per body
  (`basematerials`) waits for the `materials` worktree; Bambu Studio's project
  settings are undocumented and stay out. Field case `one-file-for-the-slicer`,
  not yet run.
- **`export_part`'s `open: true` — built 2026-09-16.** It hands the written
  file to the application the system opens its extension with (`open`,
  `xdg-open`, `start`), so an agent with no shell (Claude Desktop, ChatGPT)
  can put a 3MF in the user's slicer. The error path is measured: with no
  slicer installed, `opened: false` and `open_error` names what to install.
  The success path is not, because this machine has no slicer. Deep links
  (`bambustudio://`, `prusaslicer://`) were rejected: they download from an
  https URL on the slicer vendor's own sites (MakerWorld, Printables) and never
  open a local file.

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

### A 3D viewer inside the chat — after launch, the second announcement

MCP Apps (the first official MCP extension, 2026-01-26) lets a tool point at a
`ui://` HTML resource that the client renders in a sandboxed iframe. That
iframe can call the server's tools back. Claude web and Desktop, VS Code
Copilot, Goose and others render it. The official examples include a three.js
server, Autodesk ships its APS viewer into chats this way, and an Onshape MCP
server feeds a three.js view from glTF.

- **The case for it:** today an agent's view is a PNG line in the reply. This
  would put the orbitable part in the conversation, which is the demo
  `docs/GROWTH.md` wants for "a second thing to announce within two weeks".
- **Shape:**
  - GLB export: OCCT's `RWGltf_CafWriter` (`TKRWMesh`/`TKDEGLTF`, not linked
    yet), or written in Rust from the body meshes 3MF already uses.
  - A self-contained viewer page reusing `app/src/viewport.ts`'s look, served
    as the resource `evaluate_part` points at. The CSP is declared in
    `_meta.ui.csp`, or everything is inlined.
  - The same GLB serves `<model-viewer>` embeds and iOS AR Quick Look.
- **Risks:**
  - Client support varies (Claude Code in a terminal shows nothing).
  - A large mesh inside an iframe is a practical limit.
  - Whether a model *uses* it is a field case, not an assumption.
- **Estimate:** 3–6 days.
- **Sources:**
  - <https://blog.modelcontextprotocol.io/posts/2026-01-26-mcp-apps/>
  - <https://github.com/modelcontextprotocol/ext-apps/tree/main/examples/threejs-server>
  - <https://aps.autodesk.com/blog/embedding-aps-viewer-ai-chats-mcp-apps>

## The small win, whenever there is room for one

**Arcs in a section.** `Edge::arc` is already bound; it unlocks sealing grooves,
bearing seats and radiused shoulders, and it is exact, which is the bar an op
has to clear here. It fits the narrow reading without committing to
it, and the evidence is now a part rather than a table:
`examples/hydraulic-line.js` carries a round-bottomed groove because a torus is
the only section available, and says so in its header. Top row of
[DSL_GAPS.md](DSL_GAPS.md)'s "Still missing", item 1 in
[OP_ROADMAP.md](OP_ROADMAP.md)'s suggested order.

## Also true, and smaller than it sounds

The Codex desktop app cannot reach the MCP server at all — it fails the handshake
against a local Streamable HTTP server on macOS. A stdio shim, or a documented
`npx mcp-remote` line, is hours of work and is the only load-bearing part of "let
other agent clients use this"; the rest of that question was asked on 2026-08-07
and declined, in [ROADMAP.md](ROADMAP.md) under "Reach — which clients can talk
to this at all".

**Done.** The editor waited 350 ms after the last keystroke, chosen when a
rebuild cost 300–450 ms, where kernel time is now 76–130 ms. It is 120 ms, as
`DEBOUNCE_MS` in [`engine.ts`](../app/src/engine.ts).

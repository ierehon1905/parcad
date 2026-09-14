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
`backend.rs`, `sdf.rs`, `dsl.ts`):

1. **Parts with more than one solid** — running. The cheap end of DSL_GAPS §0's
   multi-body row: named solids in one part, never fused, measured per body.
2. **One engine.** Decided: the exact kernel becomes the only one, and the
   implicit backend (`sdf.rs`, fidget) is deleted once nothing needs it. Today it
   refuses 33 of the 41 parts in a real project folder (every edge treatment,
   loft, sweep, helix); `blend` means a different shape in it; and
   `probe_part` and `measure_wall_thickness` run on it, so they measure a part
   with its fillets dropped and call the answer an upper bound. The exact kernel
   is already interactive — median 105 ms per example, 23 of 24 under 550 ms,
   `plate-stand` at 3.3 s. The work: point probes, rays, wall thickness, renders
   and tag regions on OpenCASCADE (a solid classifier, a ray–surface
   intersection, the mesh), then speed where `tools/bench-kernel.sh` says the
   time goes (a warm worker instead of a spawn per request, reused sub-shapes,
   a coarser mesh while typing), then the deletion. The field suite has to read
   the moved tools SOUND before the old path goes.
3. **Threads that close** — diagnose the helical cut that opens at three turns
   (OP_ROADMAP §5).
4. **Curves in sections and paths** — arcs and splines in the profile type; four
   of `eval/targets/fusion360/` wait on it.
5. **A playground in the browser** — the exact kernel built for WebAssembly on
   GitHub Pages, the corpus run against that build first.
6. **A measured parts library** — fasteners, bearings, boards, devices, each
   held by eval cases, and a way for one part to import another.

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
5. **Two solids that stay two.** `check_fit` measures a reference that is
   never joined; a part printed in two halves still needs a flag and two
   evaluations. The multibody row of DSL_GAPS §0, from its cheap end.

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
   ([OP_ROADMAP.md](OP_ROADMAP.md) §5). **Threads are still refused.** A
   coaxial helical cut opens past two turns, and the cause is not diagnosed
   ([GOTCHAS.md](GOTCHAS.md)).

Already true and misreported: `preview_ready` / `exact_ready` do not apply,
because the window runs one exact kernel per evaluation and the snapshot names
it in `backend`.

## The small win, whenever there is room for one

**Arcs in a section.** `Edge::arc` is already bound; it unlocks sealing grooves,
bearing seats and radiused shoulders, and it is exact in both backends, which is
the bar an op has to clear here. It fits the narrow reading without committing to
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

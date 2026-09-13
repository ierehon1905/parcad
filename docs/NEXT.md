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
is a decision, not a task. There is no updater, and nothing has been built for a
target that is not this one, where the stripped triple suffix is the assumption
most likely to break. The `.app` has been run from outside the build tree, never
from a fresh user account.

**One hazard is free to close and worth closing.** `tauri build` has no
dependency on `tools/build-worker.sh`. A *missing* staging copy fails the build
loudly; a *stale* one silently bundles last week's kernel and measures parts
confidently with it — the exact shape of wrongness this project refuses
everywhere else, currently held by a sentence in a document rather than by the
build.

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

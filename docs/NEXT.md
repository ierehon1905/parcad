# What to do next, and in what order

Three other documents already say what is missing and why:
[ROADMAP.md](ROADMAP.md) for the agent surface, [OP_ROADMAP.md](OP_ROADMAP.md)
for geometry, [DSL_GAPS.md](DSL_GAPS.md) for what the language makes hard. None
of them says which to do *first across all three*, which is what this file is.
It holds order and scope only — where a fact lives in one of those, this links
to it rather than repeating it, because a second copy is a copy that goes stale.

The ordering argument, in one line each:

1. **Ship it** — nobody can run this on a machine that is not this one.
2. **Make it live** — the agent surface works and does not feel like it does.
3. **Go deep, not wide** — be the best tool for precision parts rather than a
   weaker Fusion.

---

## 1. Shipping — the bundle carries its kernel; nothing else about shipping works

**Done: the sidecar.** `tauri.conf.json` declares `parcad-occt-worker` as an
`externalBin` with `bundle.active: true`, `tools/build-worker.sh` stages the
triple-suffixed copy the bundler demands into `app/src-tauri/binaries/`, and
[`worker_path()`](../crates/parcad-occt/src/host.rs) accepts either the bare or
the suffixed name — Tauri 2.11 on macOS strips the suffix again, which the
comment there records as a measured fact rather than a contract. Verified the
only way it counts: `ParCAD.app` installed to `/Applications`, launched with
`PARCAD_OCCT_WORKER` unset, evaluated `examples/bracket.js` over the HTTP host
at **55074.791 mm³, 80 × 60 × 44 mm, watertight, 22 faces / 107 edges** — the
value `eval/cases/bracket.json` records. With the worker deleted from the same
bundle the evaluation refuses and names the rebuild.

**Still not shipping, and the next step costs money rather than time.** The
bundle is unsigned and un-notarised, so a second machine meets Gatekeeper before
it meets the kernel. Signing needs an Apple Developer Program membership; that
is a decision, not a task. There is also no updater, and nothing has been built
for a target that is not this one, where the stripped triple suffix is exactly
the assumption most likely to break. The `.app` has been run from outside the
build tree, never from a fresh user account.

**One hazard here is free to close and worth closing.** `tauri build` has no
dependency on `tools/build-worker.sh`. A *missing* staging copy fails the build
loudly; a *stale* one silently bundles last week's kernel, and the app it
produces measures parts confidently with it. That is the exact shape of wrongness
this project refuses everywhere else, and it is currently held by a sentence in
a document rather than by the build.

## 2. One live session the agent can drive — built

The decided design shipped as designed: `app/src-tauri/src/session.rs` beside
`service.rs`, `open_project` / `set_script` / `get_session` over MCP, viewers
pushing their document on the existing debounce, SSE for browsers and a Tauri
event for the webview, each viewer ignoring what it originated. An agent edit
is an ordinary edit — it lands in CodeMirror's undo history and Cmd-Z takes it
back; there is no lock. The account of what was built, and the echo rule that
made it hold, is in [ROADMAP.md](ROADMAP.md) under "Built: one live session the
agent can drive"; whether a model drives it is measured by
`eval/field/change-the-open-part.md`, one trial at a time.

The prerequisite this item named — one `EvaluationSnapshot` every transport
serialises — landed first and holds. The session carries the *script*, not an
evaluation, so nothing here re-opened that door.

Worth keeping from how that prerequisite landed, because this file was wrong
about it: the duplicate definition of an evaluation was never in `mcp.rs`, which
had been fixed in `9cbdd443`. It was on the far side of the UI transport, in
`main.ts`'s own `Report` interface (that file is now `state.ts` and `engine.ts`;
the frontend was ported to Preact). Both roadmaps can run a commit or two behind
the code — check before believing either, including this one.

## 3. Depth over breadth, and the shape question behind it

**The state, measured rather than assumed.** Twenty-one of the reference corpus's
Fusion documents were exported and counted. Thirteen have surfaces this kernel
cannot make; fifteen are more than one solid. Loft and Sweep each appear in six
designs — more than Revolve, which we did build. The counts and their caveats are
in [DSL_GAPS.md](DSL_GAPS.md), "What twenty-one real Fusion 360 designs actually
needed", and that section is evidence about priorities, not a specification.

**The two honest readings, which are different products.**

- **Narrow.** Machined metal: brackets, fittings, manifolds, flat faces and
  round holes. parcad is already unusually good here, the missing shapes barely
  matter, and "the only CAD an agent can use *and* verify" is a stronger
  position than a weaker Fusion. Recommended.
- **Wide.** Flowing surfaces and multiple solids. Long, serious, and a direct
  fight with tools that have had twenty years.

**This is a product call, not a technical one, and it is not made yet.** Nothing
below item 3 should be started until it is — the two readings disagree about
what is worth building next, so building either first is a bet placed early.

## The small win, whenever there is room for one

**Arcs in a section.** Today a profile is a list of straight segments, so a
radius in section has nowhere to live. `Edge::arc` is already bound. It unlocks
sealing grooves, bearing seats and radiused shoulders — everyday machined
features — and it is exact in both backends, which is the bar an op has to clear
here.

It fits the narrow reading and does not commit to it. The evidence that it is
wanted is now a part rather than a table: `examples/hydraulic-line.js` carries a
round-bottomed groove because a torus is the only section available, and says so
in its header. Top row of [DSL_GAPS.md](DSL_GAPS.md)'s "Still missing", item 6 in
[OP_ROADMAP.md](OP_ROADMAP.md)'s suggested order.

## Also true, and smaller than it sounds

One client cannot reach the MCP server at all: the Codex desktop app fails the
handshake against a local Streamable HTTP server on macOS. A stdio shim, or a
documented `npx mcp-remote` line, is hours of work and is the only load-bearing
part of "let other agent clients use this" — the rest of that question (a
terminal in the app, the UI as an agent channel) was asked on 2026-08-07 and
declined. The argument, and what the browser *is* good for, is in
[ROADMAP.md](ROADMAP.md) under "Reach — which clients can talk to this at all".

**Done.** The editor waited 350 ms after the last keystroke — chosen when a
rebuild cost 300–450 ms, where kernel time is now 76–130 ms, so the timer had
become the longer half of the latency. It is 120 ms, as `DEBOUNCE_MS` in
[`engine.ts`](../app/src/engine.ts), taken while that file was open for the
Preact port.

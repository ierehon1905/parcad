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
only way it counts: `parcad.app` copied to `/tmp`, launched with
`PARCAD_OCCT_WORKER` unset, evaluated `examples/bracket.js` over the HTTP host
at **55074.791 mm³, 80 × 60 × 44 mm, watertight, 22 faces / 107 edges** — the
value `eval/cases/bracket.json` records. With the worker deleted from the same
bundle the evaluation refuses and names the rebuild.

**Still not shipping.** The bundle is unsigned and un-notarised, so a second
machine meets Gatekeeper before it meets the kernel; there is no updater; and
nothing has been built for a target that is not this one, where the stripped
suffix is exactly the assumption most likely to break. The `.app` has been run
from outside the build tree, not from a fresh user account.

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

What this item still owes: the prerequisite it named — one `EvaluationSnapshot`
every transport serialises — landed first and holds; the session carries the
*script*, not an evaluation, so nothing here re-opened that door.

## 3. Depth over breadth, and the shape question behind it

**The state, measured rather than assumed.** Twenty-one of the author's own
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

The editor waits 350 ms after the last keystroke ([`main.ts:268`](../app/src/main.ts))
— chosen when a rebuild cost 300–450 ms, where kernel time is now 76–130 ms. You
now wait longer for the timer than for the geometry. ~120 ms roughly halves felt
latency, and it is a one-line change worth making the next time that file is open
for another reason.

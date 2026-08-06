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

## 2. One live session the agent can drive

**The state.** MCP works: an agent reaches the same `service.rs` the windows do.
But every call is stateless, `save_project` writes a file and stops, and no event
reaches the webview — so while an agent works, the open window is a correct but
stale view. Reloading is the only way to see what happened.

**Done looks like.** An agent changes the part and the window changes. If the
user disagrees, Cmd-Z takes it back the way it takes back their own typing.

**The design is already decided** and written down — session state plus a
broadcast channel, `open_project` / `set_script` / `get_session`, viewers pushing
their own document on the existing debounce, SSE for browsers and a Tauri event
for the webview, each viewer ignoring what it originated. It is in
[ROADMAP.md](ROADMAP.md) under "Decided, not built: one live session the agent
can drive". **Read that before designing anything**; the conflict rule in
particular is a product decision, not an implementation detail.

**Its prerequisite is done, and it is now the top of this list.**
`service::evaluate` produces one `EvaluationSnapshot` and all three transports
serialise it; `mcp.rs` defines no type that describes geometry. Worth knowing how
that landed, because this file said the wrong thing about it: the duplicate
definition was *not* in `mcp.rs` — that had been fixed in `9cbdd443` and the
roadmap was simply stale. It was on the far side of the UI transport, in
`main.ts`'s own `Report` interface, deriving size, volume, faces and
watertightness in TypeScript. Both roadmaps can run a commit or two behind the
code; check before believing either.

**Do not** add a lock. An agent edit is an ordinary edit; a lock would have to be
explained and undo does not.

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

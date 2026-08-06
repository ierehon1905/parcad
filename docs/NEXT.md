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

## 1. Sidecar packaging — a bundled app cannot find its kernel

**The state.** `tools/build-worker.sh` copies `parcad-occt-worker` beside the
dev binaries, and [`host.rs`'s `worker_path()`](../crates/parcad-occt/src/host.rs)
finds it there or at `PARCAD_OCCT_WORKER`. That covers development and covers
nothing else: `app/src-tauri/tauri.conf.json` has `bundle.active: false` and no
`externalBin`, so a bundled `.app` ships without the worker and every B-rep
build in it fails.

**Done looks like.** A `.app` built on a clean checkout, copied to a second
machine or a fresh user account, opens a part and measures it.

**The trap, and it is the whole job.** Tauri sidecars are declared with a plain
name and installed with the target triple appended —
`parcad-occt-worker-aarch64-apple-darwin`. `worker_path()` joins the bare name,
so a correctly-declared sidecar still will not be found. Either resolution
learns the suffixed name or the build installs both; decide which, and say in a
comment why, because the next person will hit exactly this.

**Do not** solve it by making the failure quieter. `worker_path()`'s error names
the fix, which is the house rule (CLAUDE.md, "Error messages name the fix") —
keep that standard for whatever the bundled case fails with.

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

**Its prerequisite is real work, not a chore.** `mcp.rs` builds its own flat
summary and rescans raw graph JSON for treatment nodes — a second definition of
"what an evaluation is", living in a transport file, which is the divergence
`service.rs` was extracted to stop. One `EvaluationSnapshot` that every transport
serialises and nobody redefines comes first. See
[AI_CAD_PLATFORM.md](AI_CAD_PLATFORM.md).

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

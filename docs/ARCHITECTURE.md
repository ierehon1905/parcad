# Architecture

## The intent graph is the only contract

`crates/parcad-core/src/graph.rs` defines a `Doc`: an arena of `Node`s, a
`root`, and units (always `"mm"`). Nothing in it knows how a shape is computed.
That is the whole design.

```
  script (TS)  ──build()──>  Doc (JSON)  ──┬── sdf::lower  ──> fidget tree ──> mesh, renders, regions
                                           └── occt lower  ──> TopoDS_Shape ──> mesh, edges, STEP
```

Consequences worth internalising:

- **Primitives are centred on the origin.** Placement is a separate `Translate`
  node. This keeps distance fields exact and makes symmetry the default.
- **Tags, never indices.** A `tag` on a node is the anchor for selection. In the
  implicit backend a tag resolves to a *region of the visible surface*; in the
  B-rep backend it will resolve to a *set of faces*. Same name, both times. This
  is how we dodge the topological-naming problem for as long as possible.
- **A node's meaning can differ per backend and that is allowed** — but it must
  be documented. See "blend" below.

### `blend` means two different things

| backend | how it's done | visible difference |
|---|---|---|
| implicit | smooth-minimum of the two fields | the join **bulges** by roughly `k/4` |
| B-rep | boolean, then fillet the newly-created edges | no bulge; a true fillet |

Both are defensible readings of "round this join by 6 mm". They are not the same
shape. Don't try to make the implicit one match — the whole point of the mesh
preview is that it's cheap and approximate.

## Two backends, deliberately unequal

`parcad_core::evaluate` is the implicit path and is *total*: it always returns
something. `parcad_occt::evaluate` is the exact path and is allowed to refuse.

The CLI keeps them honest by **not** pretending `--brep` is a drop-in: renders
and tag regions are raymarched from a distance field, a B-rep has none, so
`run_brep` prints a note rather than silently emitting fewer files.

## The B-rep kernel runs in a child process

`crates/parcad-occt/src/host.rs` is the reason this crate is split in half.

OCCT signals failure by throwing `Standard_Failure`, which **does not derive
from `std::exception`**. It escapes the `cxx` bridge's catch and calls
`std::terminate`. It can also segfault on degenerate input and spin for minutes
on a pathological fillet. `catch_unwind` helps with none of that.

So: the worker is a separate binary. Every outcome — including the ones OCCT
expresses by killing the process — comes back as an `OcctError` variant:

- `Rejected` — the kernel understood and said no. Actionable.
- `Crashed` — it died. `stage` is the last **breadcrumb** the worker printed to
  stderr before going down, which is the only evidence of what killed it.
- `TimedOut` — still running past the deadline (default 20 s).
- `Host` — we couldn't start it or couldn't read its reply.

For an agent that will routinely ask for a fillet larger than the material can
take, this is the difference between a bad answer and a dead session.

**The reply travels via a temp file, not stdout.** OCCT writes progress banners
to stdout — the STEP writer alone emits hundreds of kilobytes — so stdout is a
channel we neither control nor can parse.

### The feature split

`parcad-occt` builds by default as *just the host half* — no C++ at all. The
`kernel` feature pulls in `opencascade` and enables `backend.rs` and the worker
binary. The app and CLI depend on it with default features, so an everyday
`cargo build` never touches OpenCASCADE. `tools/build-worker.sh` builds the
worker and drops it beside every application binary, which is where
`host::worker_path()` looks (override with `PARCAD_OCCT_WORKER`).

## Post-condition verification instead of an allow-list

OCCT's `offset_surface` will happily return a **valid-looking but wrong** solid:
it silently drops bodies on booleans, and `offset_surface(+3)` can hand back an
inside-out shape whose next offset then runs backwards. None of this raises.

`backend.rs` therefore checks its own work. Offsetting by `d` must move every
bounding-box extreme by exactly `d` — `offset_slip()` measures the violation and
the operation refuses above `SLIP_TOLERANCE_MM` (0.05), naming the millimetre
error. This turns a class of silent wrongness into a loud refusal, which is
strictly better than an allow-list of "shapes we think are safe".

Two lowerings exist for the same reason:

- **`Offset` of a cuboid** is lowered as *grow + fillet all 12 edges to r*.
  That is not an approximation of a Minkowski sum — for a box it **is** the
  Minkowski sum, exactly, and it avoids `offset_surface` entirely.
- **`Shell`** is `solid − solid.offset_surface(−t)`. The wrapper's `hollow()`
  needs a face to open and produced a *shrunken solid* rather than a hollow one
  (measured: 64×39×22 became a solid 60×35×18 with six faces).

## Meshing: weld before you measure

OCCT triangulates **face by face**, so every shared edge arrives as two
coincident vertex copies. A perfectly closed solid then reports "NOT watertight
— 2238 bad edges". `Tessellation::weld(1e-3)` merges coincident vertices and
drops degenerate triangles; both the CLI and the app call it before computing
mass properties or stats.

## Edges are the kernel's, not inferred

The viewport draws OCCT's own edge curves rather than guessing creases in screen
space. `worker.rs::edge_curves()` keys each edge by its rounded polyline and
keeps only those bordering **two or more distinct faces** — that filter is what
removes *seam edges*, where a closed surface's parameterisation wraps. Seams are
topologically real but visually an artifact: without the filter every bore has a
line down it. For the bracket this takes 77 curves down to 67.

## The app

- `app/src/dsl.ts` is the authoring layer and lives in TypeScript, not Rust.
  That's what lets `tools/run.ts` (bun) and the webview run *the same* DSL and
  hand the same JSON to the same core.
- `app/src/viewport.ts` renders **two visually distinct looks** so you always
  know which kernel you're seeing: B-rep gets `MeshStandardMaterial`, real edge
  lines and a silhouette outline pass; implicit gets flat-shaded Lambert, a
  wireframe overlay at 0.14 opacity, a hemisphere light, and *no outline pass at
  all*. The implicit view is explicitly a **mesh preview**, not a pretend
  finished surface.
- Timings the app reports include JSON serialisation of the geometry across the
  Tauri IPC bridge, which for the bracket (~12 000 triangles + 67 edge curves)
  is a real fraction of the total.

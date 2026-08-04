# parcad

Parametric CAD for agents and people, built around one idea: **the model is a
document of intent, not a pile of geometry.**

A script builds an *intent graph* — a small JSON DAG saying "a plate, a wall,
union them with a 6 mm blend, drill four holes". Two independent kernels consume
that same graph:

| | implicit (fidget) | B-rep (OpenCASCADE) |
|---|---|---|
| what it is | signed distance field | exact surfaces and topology |
| speed | instant, never fails | ~40 ms build for a real part |
| accuracy | approximate; dual-contouring stair-steps sharp edges | exact — `box(20).scale(2)` is *exactly* 8000 mm³ |
| gives you | raymarched renders, tag-region maps, any field probe | faces, edges, STEP export, real edge curves |
| used for | agent perception and field queries | the desktop mesh preview and finished part |

Neither is a fallback for the other. The implicit backend can answer questions a
B-rep can't ("what is the distance to material at this point?"); the B-rep
backend produces geometry a slicer or a machinist will accept. The graph is the
contract between them, which is why a script written today survives a kernel
swap tomorrow.

## Reproducible setup

The committed `Cargo.lock` and `app/bun.lock` pin the Rust and JavaScript
dependency graphs. `rust-toolchain.toml` pins Rust 1.95.0; Cargo will install
that toolchain through `rustup` when needed. This source tree is intended to
build on macOS, Linux, and Windows with a C++ compiler and CMake available.
The desktop app has the usual Tauri system prerequisites: Xcode Command Line
Tools on macOS, or the platform-specific packages in the
[Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/).

Install [Bun 1.3.13](https://bun.sh/) and CMake, then check the toolchain:

```bash
rustc --version       # 1.95.0 (selected automatically by rustup)
bun --version         # 1.3.13
cmake --version
```

Install the locked frontend dependencies before running the desktop app:

```bash
cd app
bun install --frozen-lockfile
cd ..
```

Build the complete project from the repository root:

```bash
cargo build --locked --release                     # core + CLI + app host
tools/build-worker.sh                              # B-rep kernel; ~10 min cold build
```

`tools/build-worker.sh` builds the vendored OpenCASCADE kernel and places its
worker next to the release binaries. Keep `Cargo.lock`, `app/bun.lock`, and
`rust-toolchain.toml` unchanged to reproduce the same dependency inputs.

Run a part headlessly:

```bash
bun tools/run.ts examples/bracket.js > /tmp/bracket.json
./target/release/parcad /tmp/bracket.json --out out --regions
./target/release/parcad /tmp/bracket.json --brep --step out/part.step
```

Select one exact edge for an operation with a directional query — never a
kernel edge index:

```js
return body.edges(">Z and >Y and |X").fillet(2);
```

`>Z` means the topmost edge centre, `>Y` the positive-Y-most, and `|X` a
straight edge parallel to X. The desktop viewport shows temporary `edge@…` and
`vertex@…` IDs on hover and can copy a matching selector; those IDs are
diagnostic only and are not valid script input after a rebuild.

Select a geometric corner when the intent is to round or bevel all of its
incident edges together:

```js
return body.vertices(">X and >Y and >Z").expect({ count: 1 }).fillet(2);
```

The selector names the outermost positive-X, positive-Y, positive-Z vertex;
the exact backend expands that corner to its three incident B-rep edges and
constructs one rolling-ball corner result. Vertex selectors currently use
directional extrema (`>X` / `<X`) or `{ at: { x: "max", ... } }` only. They do
not yet claim Boolean provenance that the kernel cannot follow for vertices.

Click the `.fillet`, `.chamfer`, `.smooth`, or `.squircle` method name in the
desktop editor to overlay the exact input edges in gold. This is a source-to-
viewport inspection aid: it resolves the authored selector before the treatment
changes topology, and does not turn a temporary viewport ID into script input.

The reverse inspection link uses the exact history from the fillet/chamfer
builder. A final edge that the builder generated is labelled `from .fillet(…)`
or `from .chamfer(…)` on hover; its source call highlights immediately, and a
click focuses it and reveals its gold input edges. This is one-evaluation
inspection metadata, not a durable edge reference. It follows an unchanged
generated curve through placement and later operations, but does not guess when
a later operation replaces that curve.

The editor derives those source locations from the parsed code and passes them
only to the evaluation copy. The authored script and intent graph stay
unchanged, and the links work consistently in browser JavaScript and Tauri's
WebKit runtime.

For all upper rims of drilled circular holes, use a topology query instead of
listing edge IDs:

```js
return drilled
  .edges({
    generatedBy: "mount_holes",
    curve: "circle",
    role: "hole",
    adjacentTo: { faceNormal: "+z" },
  })
  .expect({ count: 4 })
  .fillet(0.8);
```

`role: "hole"` is a closed inner circular loop; combined with `+z`, it selects
the top rims only. `generatedBy` restricts that class to edges created by the
named Boolean feature, so a later unrelated circular hole does not join the
fillet set. The relation is carried through later exact Boolean operations and
refused after operations whose history is not exposed yet. `expect({ count: 4
})` makes an unexpected topology change a build error rather than silently
filleting a different set of edges.

### Edge treatments

The same stable edge selection can drive different treatments:

```js
body.edges(">Z and >Y and |X").chamfer(1);
body.edges({ role: "hole", adjacentTo: { faceNormal: "+z" } }).fillet(0.8);
body.vertices(">X and >Y and >Z").chamfer(1);
```

Equal-distance chamfers and tangent (G1) fillets are exact today. `.smooth()`
(also available as `.squircle()`) records a curvature-continuous G2 blend
request and currently fails clearly until the exact backend can construct that
surface; it is not silently approximated as a circular fillet.

Run the desktop app — **from `app/`, not the repo root**:

```bash
cd app && bun run tauri dev
```

Verify a clean checkout after setup:

```bash
cargo test --locked --workspace
cd app && bun run build
```

## Layout

```
crates/parcad-core     intent graph, SDF backend, meshing, measurement, renders
crates/parcad-occt     B-rep backend — host half + worker half, feature-split
crates/parcad-cli      headless driver: graph in, STL/PNG/STEP/report out
app/                   Tauri 2 desktop app (three.js viewport, CodeMirror editor)
app/src/dsl.ts         the authoring DSL — shared by the app and tools/run.ts
vendor/opencascade     forked wrapper crate; see PARCAD-CHANGES.md
tools/                 build-worker.sh, bench-kernel.sh, run.ts
cmake/                 OCCT toolchain file (this is a performance fix, see docs)
```

## Docs

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how the pieces fit and why
- [docs/GOTCHAS.md](docs/GOTCHAS.md) — traps that have already cost a day each
- [docs/ROADMAP.md](docs/ROADMAP.md) — known gaps and what's next
- [CLAUDE.md](CLAUDE.md) — conventions, for agents and people alike

## Licensing

Our crates are MIT OR Apache-2.0. **`vendor/opencascade` is LGPL-2.1** (forked
from the crates.io `opencascade` 0.2.0), and OpenCASCADE itself is LGPL-2.1 with
an exception. That's fine for the current dynamic/static-with-source situation
but is a real constraint on any closed redistribution — decide it deliberately
before shipping binaries.

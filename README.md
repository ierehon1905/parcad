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
| used for | the mesh preview, agent perception | the finished part |

Neither is a fallback for the other. The implicit backend can answer questions a
B-rep can't ("what is the distance to material at this point?"); the B-rep
backend produces geometry a slicer or a machinist will accept. The graph is the
contract between them, which is why a script written today survives a kernel
swap tomorrow.

## Quickstart

```bash
cargo build --release                              # core + CLI + app host
tools/build-worker.sh                              # the B-rep kernel (slow, ~10 min cold)
```

Run a part headlessly:

```bash
bun tools/run.ts examples/bracket.js > /tmp/bracket.json
./target/release/parcad /tmp/bracket.json --out out --regions
./target/release/parcad /tmp/bracket.json --brep --step out/part.step
```

Run the desktop app — **from `app/`, not the repo root**:

```bash
cd app && bunx tauri dev
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

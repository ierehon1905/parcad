# ParCAD

Parametric CAD you write as code — for people, and for agents.

**Status: 0.0.1, experimental.** One author so far. The DSL still moves between
versions, and while every operation here is checked against measured geometry
([eval/cases/](eval/cases/)), nothing about this project has been through the
years of abuse that makes a CAD kernel trustworthy. Measure a part before you
machine it.

![A bracket built by the script below, in the parcad viewport](docs/images/bracket.jpg)

```js
const plate = box(80, 60, 8).tag("plate");
const wall = box(8, 60, 40).at(-36, 0, 20).tag("wall");

const body = union(plate, wall, { blend: 6 });          // the blend is the fillet
const drilled = body.cut(...grid(2, 2, 36, 40).map(([x, y]) => cylinder(3, 32).at(x, y)));

return drilled
  .edges({ curve: "circle", role: "hole", adjacentTo: { faceNormal: "+z" } })
  .expect({ count: 4 })                                  // a build error, not a surprise
  .fillet(0.8);
```

You never name an edge by index. You describe it — *the circular rims that open
onto the top face* — and the description still works after you move a hole.

## Two kernels, one model

Your script builds a small JSON graph, and two kernels read it: an implicit one
(signed distance fields, instant) and an exact one (OpenCASCADE — real faces,
edges, STEP export). Neither is a fallback for the other, and the graph is what
keeps today's script working after tomorrow's kernel swap.

![The same manifold block cut open on a plane, cut faces in orange](docs/images/manifold-section.jpg)

Parts get measured, not assumed — volume, wall thickness, whether two bores
actually meet. An agent gets the same numbers over MCP, with no screen to look at.

It is aimed at small mechanical parts — brackets, flanges, manifolds, enclosures,
heat sinks, the things you print or machine one of. [examples/](examples/) is
twenty-two of them.

## Why not CadQuery, build123d or OpenSCAD

Those are good, and much older. Three things here are different. Selectors are
*descriptive and checked*: `.expect({ count: 4 })` turns a selector that silently
started matching three edges into a build error, which is the failure mode that
makes code-CAD fragile. Two kernels read one graph, so a sketch-fast implicit
preview and an exact B-rep for STEP export are the same model rather than two
projects. And measurement is a first-class output, not something you eyeball in a
viewport — which is what makes the MCP surface real rather than a wrapper.

## Build it

Development is on macOS. On Linux, CI builds and tests the kernel crates, the
editor-side tests and the grader on every push — but the exact kernel and the
desktop window are still untried there, so treat OpenCASCADE and the app as
unverified on Linux and patches welcome. Windows is not supported: the B-rep
worker's process handling has no Windows arm.

You need [Rust](https://rustup.rs/) (the toolchain is pinned; rustup honours it),
[Bun](https://bun.sh/), CMake, a C++ compiler, `patch(1)` and Python 3.11+. A
clone is a 28 MB pack that expands to 144 MB, most of it a vendored OpenCASCADE
tree, and a full build wants around 20 GB free.

```bash
cd app && bun install --frozen-lockfile && cd ..   # first: the Rust build shells out to bun
cargo build --locked --release
tools/build-worker.sh     # the exact kernel — about 10 minutes, once
cd app && bun run tauri dev
```

`cargo build` produces a *dev* app whatever the profile — Tauri's switch is the
`custom-protocol` feature its CLI adds, not `--release` — so run the window
through `tauri dev` or `tauri build`, not by launching the bare binary. And a
root `cargo build` deliberately does not build OpenCASCADE; `tools/build-worker.sh`
is what does.

```bash
tools/check.sh            # everything that gates a change (~5 min)
tools/check.sh --fast     # the kernel crates and the editor tests (~4 s)
```

Or headless:

```bash
bun tools/run.ts examples/bracket.js > /tmp/bracket.json
./target/release/parcad /tmp/bracket.json --out out --brep --step out/part.step
```

The app serves its UI and an MCP endpoint on <http://127.0.0.1:4242>. Parts live
in `~/Documents/parcad` as plain `.js` files you can edit anywhere.

```bash
claude mcp add --transport http parcad http://127.0.0.1:4242/mcp
```

Point an agent at it and start with the `read_docs` tool — it hands over the
whole language in one call.

## More

- [examples/](examples/) — twenty-two parts, from a bracket to a hydraulic manifold
- [CONTRIBUTING.md](CONTRIBUTING.md) — the gate, and what a change has to satisfy
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how the pieces fit, and why
- [docs/GOTCHAS.md](docs/GOTCHAS.md) — traps that have already cost a day each
- [docs/NEXT.md](docs/NEXT.md) — what's missing, in order

## Licensing

ParCAD's own code is MIT or Apache-2.0, at your option. It is built on components
that are not: OpenCASCADE and its Rust bindings under `vendor/` are LGPL-2.1, and
the implicit kernel, [fidget](https://github.com/mkeeter/fidget), is MPL-2.0.
Building and running from source is unencumbered; **redistributing a binary
carries obligations** that [NOTICE.md](NOTICE.md) spells out — read it first.

This software makes use of facilities provided by the Open CASCADE Technology
software.

# parcad

Parametric CAD you write as code, for people and for agents. The model is a
document of intent — not a pile of geometry.

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
onto the top face* — and the same description keeps working after you move a
hole or add another one.

## Two kernels, one model

A script builds a small JSON graph. Two independent kernels read it: an
implicit one (signed distance fields, instant, answers "how much material is
at this point?") and an exact one (OpenCASCADE — real faces, edges and STEP
export). Neither is a fallback for the other, and the graph is what keeps a
script written today working after a kernel swap tomorrow.

![The same manifold block cut open on a plane, cut faces in orange](docs/images/manifold-section.jpg)

Parts get measured, not assumed: volume, wall thickness, whether two bores
actually meet. That is also what an agent gets — over MCP, from the running
app, with no screen to look at.

## Build and run

Needs [Rust](https://rustup.rs/) (the toolchain is pinned), [Bun](https://bun.sh/),
CMake and a C++ compiler.

```bash
cargo build --locked --release      # kernel, CLI, app host
tools/build-worker.sh               # the exact kernel — ~10 min the first time
cd app && bun install --frozen-lockfile && bun run tauri dev
```

Or headless:

```bash
bun tools/run.ts examples/bracket.js > /tmp/bracket.json
./target/release/parcad /tmp/bracket.json --out out --brep --step out/part.step
```

The running app also serves its UI and an MCP endpoint on
<http://127.0.0.1:4242>. Parts live in `~/Documents/parcad` as plain `.js`
files you can edit anywhere.

```bash
claude mcp add --transport http parcad http://127.0.0.1:4242/mcp
```

The first tool to call is `read_docs`, which is the whole language and the same
notes on what the kernel refuses that this repository keeps in `docs/`. If the
server sits at `⏸ Pending approval` and no trust dialog ever appears, that is a
workspace inheriting trust from its parent rather than anything to do with
parcad — docs/GOTCHAS.md has the fix.

## More

- [examples/](examples/) — nineteen real parts, from a bracket to a hydraulic manifold
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how the pieces fit, and why
- [docs/GOTCHAS.md](docs/GOTCHAS.md) — traps that have already cost a day each
- [docs/ROADMAP.md](docs/ROADMAP.md) — what is missing

MIT or Apache-2.0, except `vendor/opencascade`, which is LGPL-2.1.
See [NOTICE.md](NOTICE.md).

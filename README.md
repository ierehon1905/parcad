# parcad

Parametric CAD you write as code — for people, and for agents.

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

## Run it

You'll need [Rust](https://rustup.rs/), [Bun](https://bun.sh/), CMake and a C++ compiler.

```bash
cargo build --locked --release
tools/build-worker.sh     # the exact kernel — about 10 minutes, once
cd app && bun install --frozen-lockfile && bun run tauri dev
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

Start with the `read_docs` tool — it hands over the whole language in one call.

## More

- [examples/](examples/) — twenty-one parts, from a bracket to a hydraulic manifold
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — how the pieces fit, and why
- [docs/GOTCHAS.md](docs/GOTCHAS.md) — traps that have already cost a day each
- [docs/NEXT.md](docs/NEXT.md) — what's missing, in order

MIT or Apache-2.0, except `vendor/opencascade`, which is LGPL-2.1.
See [NOTICE.md](NOTICE.md).

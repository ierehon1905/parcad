# The playground: the exact kernel in a browser tab

A static site where a visitor edits a ParCAD part in the real editor and it is
built by the same OpenCASCADE, patched the same way, running the same Rust — the
native worker's `parcad_occt::serve::run` and `parcad_evaluation` — compiled to
WebAssembly and running in a Web Worker. No server, no account.

## Build it

Pinned, and everything installed outside the repository:

| tool | version | from |
|---|---|---|
| Emscripten SDK | 6.0.9 | <https://github.com/emscripten-core/emsdk> (`./emsdk install 6.0.9 && ./emsdk activate 6.0.9`) |
| Rust | 1.97.1 (`rust-toolchain.toml`) with the `wasm32-unknown-emscripten` target | `rustup target add wasm32-unknown-emscripten` |
| Node | 22 | only to run the corpus against the build |

```bash
EMSDK=/path/to/emsdk playground/build-kernel.sh          # OpenCASCADE, then both kernels
PARCAD_OCCT_WORKER=$PWD/playground/node-worker.sh \
  cargo run -q --release -p parcad-eval                  # the corpus, against the wasm build
```

`build-kernel.sh` stages `vendor/occt-sys/OCCT` with `vendor/occt-sys/patches`
applied, configures it with the switches `vendor/occt-sys/build.rs` passes and
the toolchain in `occt-emscripten.cmake`, compiles only the toolkits
`vendor/opencascade-sys/build.rs` links, and then builds two binaries into
`target/wasm`:

- `node/parcad-occt-worker.js` + `.wasm` — the worker proper, under Node with
  the real filesystem, speaking the stdin/stderr protocol `host.rs` drives. That
  is how the eval corpus measures the WebAssembly build without a line of
  harness changed.
- `web/parcad-wasm.js` + `parcad_wasm.wasm` — `crates/parcad-wasm`, the module a
  page loads: one exported call per HTTP route the host has.

Measured on an M-series Mac: OpenCASCADE for wasm, 4651 translation units, 306 s
cold; the Rust half, 48 s.

**Relinking with another OpenCASCADE**, which LGPL-2.1 section 6 asks us to make
possible: `OCCT_SOURCE=/your/occt EMSDK=… playground/build-kernel.sh`, then the
site build below. The patches in `vendor/occt-sys/patches` are applied to
whatever tree `OCCT_SOURCE` names.

## The site

```bash
playground/prebuild.sh                 # record the first part (optional, and worth it)
cd app && bun install --frozen-lockfile
bun x vite build --mode playground     # app/dist-playground, for https://<owner>.github.io/parcad/
bun x vite preview --mode playground   # or any static server, with the files under /parcad/
bun x vite --mode playground           # the same, live, while editing the frontend
```

`prebuild.sh` evaluates the part the playground opens first and writes the
result to `target/playground/`, which the build ships beside the kernel. A
visitor then has the part on screen about 3 s after navigating, instead of
waiting out the kernel's download *and* a build — the twisted planter is 5 s of
geometry in a tab. It costs 10 MB, near 3 MB gzipped, against the kernel's own
6.3 MB.

Nothing about it is a claim: the page draws the shipped build, says
"built before this page was served, rebuilding it here" while it does, and
replaces it with what the kernel in the tab measures for the same script. It is
offered only for that part, only unedited, and only once. Skip the script and
the site still builds; the first visit is simply slower. Re-run it whenever the
part changes — a recording for a script that has moved on is ignored, not
shown.

`PARCAD_PLAYGROUND_BASE` changes the base path; `PARCAD_KERNEL_DIR` points at a
kernel built somewhere other than `target/wasm/web`. The build copies the kernel
under `kernel/<content hash>/`, so a deploy never pairs one build's JavaScript
with another's module, and ships `licenses/` (NOTICE.md says why).
`.github/workflows/playground.yml` does all of this on a manual dispatch — the
corpus against the WebAssembly worker first — and deploys with
`actions/deploy-pages`.

What a visitor gets: 25 MB of files, of which the page itself is 1.2 MB of
JavaScript (361 KB gzipped) and paints before the kernel starts downloading. The
kernel is 19.7 MB, 6.4 MB as GitHub Pages gzips it. On a local static server the
first part is on screen 650 ms after navigation; throttled to 40 Mbit/s with no
compression it took 5.6 s, and at that speed the gzipped file is about 1.3 s of
it. Checked in headless Chrome and headless Firefox against `python3 -m
http.server` serving the files under `/parcad/` (Firefox: 350 ms of kernel time
for the bracket, the same 6822 triangles); not yet in Safari or on a phone.

**A visitor can** open any of the 28 seed parts, edit them with the real editor
and op palette, see every change built by the exact kernel with the report,
section view, edge and face inspection, and the gold treatment-target preview;
read refusals that name the fix; save parts, make new ones and folders, rename
and delete them, all in that browser's storage; and export STEP and STL as
downloads.

**A visitor cannot** reach MCP (there is no endpoint, so the chip never shows),
share a live session with another window or an agent, keep parts as files or see
them anywhere but that browser, or give a heavy part more than 60 s. Nothing
leaves the tab: scripts run in the page as they do in the desktop editor, and
the kernel runs in a Web Worker beside it.

## What had to change for WebAssembly

- **Exceptions are native Wasm EH** (`-fwasm-exceptions`), because Rust's
  `wasm32-unknown-emscripten` target unwinds with it and the two must agree for a
  `Standard_Failure` to reach the fillet boundary's catch. Every refusal case in
  the corpus refuses with the same words. JavaScript-based `-fexceptions` would
  mean a nightly compiler and a rebuilt standard library (`-Zemscripten-wasm-eh=false`),
  and it is the slower and larger of the two. The encoding is Emscripten's
  default legacy one rather than `exnref`, which browsers have run since
  Chrome 95, Firefox 100 and Safari 15.2.
- **`OCC_CONVERT_SIGNALS` is undefined**: its `setjmp` inside a Wasm-EH `try`
  made clang emit a function V8 refuses. Nothing in parcad installs the signal
  handler it serves, and wasm has no signals. docs/GOTCHAS.md has this and three
  other build traps found here.
- **Single-threaded.** OCCT is built without TBB, and the worker never asks
  BRepMesh for parallelism, so there is no `SharedArrayBuffer` and no
  cross-origin-isolation header to need — which GitHub Pages cannot send.
- **Memory** grows from 64 MB up to the 4 GB wasm32 ceiling; the shadow stack is
  16 MB because OpenCASCADE recurses deeply.

## Why this and not a ready-made OpenCASCADE for the web

| option | what it is | why not |
|---|---|---|
| [opencascade.js](https://github.com/donalffons/opencascade.js) | Emscripten build of OCCT 7.x with Embind bindings for JavaScript | parcad's kernel is 4,600 lines of Rust against its own wrapper and two OCCT patches; using it means rewriting `backend.rs` in JavaScript, a second definition of every operation, on an older OCCT |
| [replicad](https://github.com/sgenoud/replicad) | a TypeScript CAD library shipping a trimmed OCCT wasm, now OCCT 8, in a no-exceptions build (19 MB) and a native-Wasm-EH build (22 MB) | the same rewrite; its with-exceptions build is the evidence that native Wasm EH is the affordable way to keep `Standard_Failure` catchable |
| [occt-wasm](https://github.com/andymai/occt-wasm) | OCCT 8.0.1, `-fwasm-exceptions`, `-flto`, `wasm-opt -O4`, ~4.5 MB brotli, structured errors | a TypeScript API over OCCT again; its size is what this build lands at too (4.3 MB brotli) without LTO |
| CascadeStudio, bitbybit | editors built on opencascade.js | products on the first row, not a kernel to reuse |
| CadQuery / build123d on Pyodide | OCP (pybind11 OCCT) under Pyodide | a Python runtime and its own OCCT build to carry, for none of parcad's code |
| **this** | OCCT and parcad's own Rust compiled together by Emscripten for `wasm32-unknown-emscripten` | one definition of an evaluation, the patched kernel, and the corpus runs against it unchanged |

`wasm32-unknown-emscripten` rather than a C++-only module with Rust on
`wasm32-unknown-unknown`: the cxx bridge, the `cc` crate and OCCT all compile
with emcc as they do natively, and Rust's standard library on this target
already unwinds with Wasm exceptions. Threads were left out rather than served
through a `coi-serviceworker` shim, because nothing in the worker is parallel.

## Is it the same kernel? The corpus says so

`PARCAD_OCCT_WORKER=playground/node-worker.sh cargo run -q --release -p parcad-eval`:

```
113 passed, 0 failed, 0 skipped, 0 known defect(s)     # 67 s; native 27 s
```

Every numeric difference between the two builds was found by recording the whole
corpus with `--update` under each and diffing the files. Faces, edges, curves,
bodies, voids and watertightness are identical in every case; so is every
refusal. Only numbers read off the tessellation move:

| case | volume | area | triangles |
|---|---|---|---|
| bent-tube | +0.0001% | +0.0000% | 5002 → 5006 (+0.1%) |
| blend-runs-off-the-edge | -0.0000% |  | 4218 → 4208 (-0.2%) |
| cast-foot | -0.0002% | +0.0000% | 3582 → 3756 (+4.9%) |
| circle-section-ring | +0.0024% | +0.0011% | 21066 → 21106 (+0.2%) |
| curve-edges-by-kind | +0.0001% |  | 6676 → 6830 (+2.3%) |
| device-body | +0.0000% | +0.0000% | 12244 → 12380 (+1.1%) |
| domed-revolve | +0.0000% |  | 5470 → 5472 (+0.0%) |
| drafted-boss |  |  | 340 → 356 (+4.7%) |
| ellipsoid | +0.0019% | +0.0025% | 26884 → 27926 (+3.9%) |
| enclosure | +0.0000% | +0.0000% | 3976 → 3984 (+0.2%) |
| helical-groove | +0.0034% | -0.0004% | 8454 → 8526 (+0.9%) |
| helical-spring | -0.0058% | -0.0026% | 61408 → 62958 (+2.5%) |
| left-hand-hook | +0.0117% | +0.0018% | 13456 → 13516 (+0.4%) |
| loft-frustum |  |  | 3764 → 3868 (+2.8%) |
| loft-smooth | +0.0009% | +0.0000% | 7174 → 7124 (-0.7%) |
| offset-box | +0.0000% | +0.0001% | 3468 → 3484 (+0.5%) |
| parabola-bezier | -0.0043% |  | 450 → 440 (-2.2%) |
| pipe-tee | -0.0000% | +0.0000% | 12376 → 12684 (+2.5%) |
| plate-stand | -0.0001% | +0.0000% | 155700 → 156600 (+0.6%) |
| re-entrant-loft |  |  | 634 → 2240 (+253.3%) |
| revolved-fillet-shoulder | +0.0003% | +0.0004% | 4170 → 4204 (+0.8%) |
| screw-top-jar | +0.0001% | +0.0000% | 13858 → 13754 (-0.8%) |
| section-m10-bolt-and-nut | +0.0003% | +0.0000% | 21872 → 21736 (-0.6%) |
| spiral-horn | +0.0016% | +0.0007% | 61130 → 61666 (+0.9%) |
| square-coil | +0.0001% |  | 6348 → 6972 (+9.8%) |
| swept-stadium-bend | +0.0022% | +0.0008% | 3662 → 3692 (+0.8%) |
| tapered-strand | +0.0037% | +0.0007% | 29856 → 30072 (+0.7%) |
| thread-bolt-and-nut |  |  | 17622 → 17378 (-1.4%) |
| thread-m8-20-turns | -0.0000% | -0.0001% | 31368 → 30988 (-1.2%) |
| thread-m8-3-turns |  |  | 5064 → 5008 (-1.1%) |
| thread-m8-8-turns | -0.0001% | -0.0003% | 12854 → 12648 (-1.6%) |
| torus | +0.0100% | +0.0027% | 21574 → 21798 (+1.0%) |
| twisted-loft | -0.0030% | -0.0164% | 3988 → 4132 (+3.6%) |
| untitled2-v1 | -0.0000% | +0.0005% | 284564 → 285100 (+0.2%) |
| untriangle-v3 | +0.0100% | -0.0146% | 12688 → 13200 (+4.0%) |
| wash-bottle | +0.0004% | +0.0001% | 148748 → 149566 (+0.5%) |

Plus bed contact on `ellipsoid` (−0.22%) and `untriangle-v3` (+0.54%), both
inside the tolerances their cases already carried for GCC on x86_64, and 0.001 mm
of size on `helical-groove` and `left-hand-hook`. The 77 other cases record
identical numbers.

One case needed a tolerance: `re-entrant-loft`'s triangle count, whose planar
walls mesh to two triangles natively and a few hundred here, with volume, area
and topology exact in both. Its `why` says why, and docs/GOTCHAS.md, "A planar
wall meshes two ways", has the instrumented trace behind it.

## How big, how fast

| | raw | gzip -9 | brotli -11 |
|---|---|---|---|
| `parcad_wasm.wasm` (what a browser downloads) | 19.67 MB | 6.33 MB | 4.30 MB |
| `parcad-wasm.js` (Emscripten glue) | 69 KB | 20 KB | 18 KB |
| native worker, arm64, for scale | 29.9 MB | 11.2 MB | 7.6 MB |

`wasm-opt -Oz` over Emscripten's `-O3` link saves 5.6% raw and 2% gzipped and
makes the brotli file larger; not used.

One cold build per process, median of five, the same request to each build:

| part | native build / mesh / wall | wasm build / mesh / wall |
|---|---|---|
| `examples/bracket.js` | 57 / 8 / 85 ms | 184 / 32 / 309 ms |
| `examples/plate-stand.js` | 2800 / 342 / 3302 ms | 3960 / 750 / 4995 ms |
| `examples/screw-top-jar.js` (threads, two bodies) | 326 / 125 / 2066 ms | 1247 / 234 / 5739 ms |

Under Node, wall time includes starting the runtime and compiling the module.
In headless Chrome, the bracket's first build in a fresh worker reports 263 ms
of kernel time against the native host's 133 ms, and its snapshot is equal to
the native host's in every field but that one.

A refusal whose number comes from a wrong solid can differ: `box(10,10,10)
.edges(">Z").fillet(8)` returns a shape reaching 4.86 mm outside its box natively
and 4.70 mm in wasm. Both refuse with the same fix — the largest radius measured
to build, 4.75 mm, is the same.

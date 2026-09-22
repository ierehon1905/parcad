# Gotchas

Written for maintainers and coding agents already working in this repository.

Things that cost real time. Each one was silent — everything "worked".

## Build & tooling

### OpenCASCADE compiles at `-O0` unless you stop it

The single largest performance bug in the project so far. The `cmake` crate
resolves `CMAKE_BUILD_TYPE` to `Debug` for `occt-sys` *even under
`cargo build --release`*: the Rust half is optimised, the geometry kernel
underneath it is not, and nothing says so. Enclosure build time was 203 ms; it
should be 43.

Fixed by `cmake/occt-toolchain.cmake`, wired in via `CMAKE_TOOLCHAIN_FILE` in
`.cargo/config.toml`. Two traps inside the fix:

- **`CXXFLAGS=-O2` does nothing.** cmake-rs's `skip_arg` strips every `-O`/`/O`/
  `-g` argument from forwarded compiler flags *on purpose*. A toolchain file is
  the one hook it passes through untouched.
- **We force `-O2` inside the Debug configuration rather than switching to
  Release.** OCCT's Release config defines `-DNo_Exception`, which turns
  `Standard_Failure` raises into no-ops — it converts our catchable geometry
  errors into undefined behaviour. Never enable it.

`tools/build-worker.sh` greps the generated `flags.make` and warns loudly if the
optimisation didn't take. Trust that check, not the config file.

Measured (median of 7):

| | `-O0` | `-O2` | `-O3` |
|---|---|---|---|
| enclosure build / mesh | 203 / 22 ms | 43 / 4 ms | 44 / 4 ms |
| bracket build / mesh | 197 / 181 ms | 39 / 28 ms | 39 / 28 ms |
| process floor | 62 ms | 11 ms | 11 ms |
| worker binary | 34.3 MB | 25.5 MB | 26.7 MB |

`-O2` is chosen: identical speed to `-O3`, 1.2 MB smaller. All 11 regression
cases produced byte-identical geometry after the change.

### `cargo test --workspace` compiled OpenCASCADE a second time

A cold `tools/check.sh` built OCCT twice, in two `occt-sys-*` directories with
different hashes — 7 GB and five minutes each. Same features; different *unit*.
`occt-sys` reaches the worker build only as a `[build-dependencies]` entry of
`opencascade` and `opencascade-sys`, compiled for the host under the
build-override profile; `--workspace` also selects it as a member in its own
right, a target-side unit whose build script is the whole OpenCASCADE compile.
The crate has no tests, so the gate now passes `--exclude occt-sys`. A crate that
is *both* a member and somebody's build-dependency gets two build-script runs
whenever both are selected, and `cargo tree -e features` will not show the
difference because there is none.

### A missing `capabilities/` rebuilt the app on every single build

`cargo build` with nothing changed took 16 s, every time. Not compilation:
`tauri_build::build()` emits a `rerun-if-changed` for `app/src-tauri/capabilities`,
and cargo treats a `rerun-if-changed` on a *missing* path as permanently stale.
That re-ran the `bun build` of the DSL bundle, which invalidated `parcad-app` and
everything downstream. The directory now exists and holds exactly one capability,
`session.json`, which lets the main window listen for the live-session broadcast;
anything further would grant permissions the app does not need. A no-op build is
0.2 s. Cargo will not tell you this; ask it:

```bash
CARGO_LOG=cargo::core::compiler::fingerprint=info cargo build --release 2>&1 | grep dirty
```

### Run the app from `app/`, not the repo root

`bunx tauri dev` at the repo root fetches `tauri` from npm and dies building
`sharp`/`vips`. And **don't** use `bunx --bun tauri dev` — that fails with
"could not determine executable to run for package tauri".

```bash
cd app && bun install --frozen-lockfile && bun run tauri dev
```

### `cargo build --release` builds a *dev* app: the window is blank, the browser is fine

A binary from a plain `cargo build --locked --release -p parcad-app`, launched
directly, brings up the HTTP host and a desktop window whose webview never runs
the frontend — no evaluation, no `preview.png`, no session pushed. A browser tab
against the *same process* works perfectly. That asymmetry reads as a broken
webview and is nothing of the kind.

**Tauri's dev/production switch is a Cargo feature, not the cargo profile.**
From `tauri-macros`:

```rust
dev: cfg!(not(feature = "custom-protocol")),
```

The tauri CLI adds `custom-protocol` to the `cargo build` it runs; nothing else
does. Without it the binary is a *dev* binary regardless of `--release`, and two
things follow, both from `tauri-codegen`'s `context.rs`:

- `dev && dev_url.is_some()` embeds `EmbeddedAssets::default()` — i.e. **nothing**.
  The window loads `devUrl` (`http://localhost:1420`), where nothing is listening.
- `with_config_parent(...)` bakes in the source dir, so the asset resolver falls
  back to reading `app/dist` **off disk**. That is the only reason the HTTP host
  serves a UI at all — and it means such a binary is not relocatable.

Measured on `747ed26b`, scratch `PARCAD_PROJECTS_DIR`, bare launch, 15 s:

| binary | window runs frontend | `/api/session` | `preview.png` | size |
|---|---|---|---|---|
| `cargo build --release` | no | `revision: 0` | 0 | 13.84 MB |
| …same, with vite up on 1420 | **yes** | `bracket` | 1 | — |
| `cargo build --release --features tauri/custom-protocol` | **yes** | `bracket` | 1 | 15.07 MB |
| `tauri build` bundle (`ParCAD.app`) | **yes** | `bracket` | 1 | 14.86 MB |

The +1.2 MB is the frontend the dev binary does not carry. Hiding `app/dist`
makes the dev binary's own HTTP host answer `404 … no frontend bundle is
embedded in this binary` — the same fact from the other side.

Two dead ends, eliminated by measurement: it is not the `.app` bundle or window
activation (the tauri-built binary works inside a hand-made minimal `.app`, the
cargo-built one fails inside the real `ParCAD.app`, and
`NSRunningApplication.activate()` returns `true` and changes nothing), and not
release-vs-debug (`tauri build --debug --no-bundle` works because the *CLI*
builds it; a plain debug `cargo build` fails the same way).

`warn_if_the_window_awaits_a_dev_server` in `lib.rs` now says so at startup. The
WebKit log misleads: the page load *completes*, then `view visibility state
changed 1 -> 0` throttles the web process to background — the consequence of
loading a dead URL, not the cause.

### A second app instance silently has no browser UI

The UI port is held by whichever instance bound it first. The second one prints
why and keeps its desktop window working — but a browser tab is then talking to
the *first* instance's kernel. Read the app's stderr; it names the fix:

```bash
PARCAD_HTTP_PORT=4243 cargo run -p parcad-app
```

A browser that loses the host mid-session shows "the parcad desktop process is
not answering" and keeps the last good geometry on screen. That is the app having
exited, not a failed evaluation — `tauri dev` restarting on a rebuild is the
usual cause.

### The MCP server is one host, however the client reaches it

There is one host per machine on port 4242. The desktop app, `parcad serve`
or `parcad mcp` provides it. `parcad mcp` is the stdio server a client launches,
and it is a relay, not a second host: it forwards to the host that is already
running, and only when none is running does it host one itself, for as long as
the client stays connected. That is the default route (the plugin, the MCP
bundle, `claude mcp add parcad -- parcad mcp`) and needs nothing running
beforehand.

A client that connects by URL cannot start anything, so it only connects while
some host is up — `brew services start parcad` keeps one up from login:

```bash
claude mcp add --transport http parcad http://127.0.0.1:4242/mcp
```

Whichever process got 4242 first owns the session. Open the desktop app while a
relay or the service holds the port, and the window works but its HTTP host
fails to bind — the agent is then editing the other process, not the window.
Quit one of them.

A `.mcp.json` in a workspace needs that workspace **explicitly** trusted, and a
workspace under an already-trusted parent inherits trust without the dialog ever
firing — so it can never *become* explicitly trusted, and the server stays
`⏸ Pending approval` forever. Not parcad's bug, and it reads exactly like one;
the fix is `hasTrustDialogAccepted: true` for that path in `~/.claude.json`.

### `cargo build` needs bun, because the sandbox embeds the DSL

`build.rs` shells out to `bun build` to compile `app/src/dsl.ts` into the script
sandbox. Not a new requirement — the frontend already needed bun — but the
failure now happens during `cargo build` rather than at `bun run tauri dev`. The
panic names the fix.

### A script's budget is counted, not timed

The sandbox used to stop a script after 5 s of wall clock. That is a
determinism bug: a script that took 4.0 s idle passed 9 of 10 runs with the
machine at load 7 and 0 of 10 under twenty `yes` processes, and the lamp that
motivated it (2.5 s idle) failed 2 of 3 under the same load. A debug build
tripped it on scripts that finish in microseconds.

`script.rs` now counts QuickJS's own interrupt polls. The interpreter decrements
a per-context counter on every call and every loop back-jump (and every 10 000
regex backtracking steps) and polls the handler when it reaches zero, so one
poll is 10 000 steps and the same bytecode on the same input polls the same
number of times. The same near-limit script then passed 10 of 10 idle and 10
of 10 under the hog; one just over the budget failed 10 of 10 both ways.

What the count does *not* see is arithmetic between calls, so steps per
second varies by workload: about 110 M/s for a bare `Math.sqrt` loop, 26 M/s
for the lamp's reaction-diffusion body, on an M-series machine. The default
600 M steps is therefore 5 s of the one and 23 s of the other. The clock
backstop (120 s per budget multiple, at most 600 s) exists for a script
heavier per step than both; it is the one limit left that load can move.
A native call that never polls cannot be stopped by either — the 64 MB cap is
what bounds those.

The budget belongs to the source, `scriptBudget(n)`, rather than to a call's
`timeout_s`: a saved part is also built by the CLI, the picker's thumbnail and
`check_fit`, none of which a caller passes a number to.

**Native helpers must match their JavaScript to the bit.** The reaction-diffusion
lamp spent 75% of its 2.5 s in its field loop and 18% in segment-crossing tests,
behind a spatial grid it had to write itself. `simulateReactionDiffusion`,
`outlineCrossings` and `outlineGaps` now run in Rust in the sandbox
(`generative.rs`) and as JavaScript everywhere else (`dsl.ts`), and the editor
and MCP must build the same part from one script. So both sides do the same
float operations in the same order, use `sqrt` where a script would reach for
`hypot`, and `native_helpers_give_the_same_bits_as_their_javascript` compares
them. The lamp rewritten on them builds the same graph under `bun tools/run.ts`
as in the sandbox. Native work is charged to the budget before it runs, one step
per cell update or pair test, because a native call cannot be interrupted, and a
charge past the budget is refused even if the script catches the throw.

**A script's result is kept by its source, unless it read the clock.** Every
MCP tool that takes a script runs it first, so an export after an evaluation,
or a second evaluation asking for views, paid the lamp's 2.5 s again (a
repeated `evaluate_part` took 2.95 s with the kernel build already reused; now
0.34 s, and the export after it 4.6 s to 1.5 s). `script.rs` now keeps each
result by its exact source. That is sound only because nothing but the source
decides the answer: the realm is empty, the budget is counted, and the three
things in it that differ between runs — `Math.random`, which QuickJS seeds
from the clock, `Date` and `performance` — are wrapped to mark the run, and a
marked run is never kept. A budget refusal is kept (the count is a fact of the
source); a clock-backstop refusal is not. A part that wants randomness should
seed its own generator, which is what every generative part here already does.

**A graph's numbers can land one ulp off on the way into Rust.** `serde_json`
without its `float_roundtrip` feature parses some shortest-form doubles to a
neighbour. Both routes (webview IPC and sandbox) parse the same strings, so they
agree with each other, but not always with the script: comparing a sandbox
graph with the JSON `bun tools/run.ts` printed shows last-digit differences
until both are parsed by serde. QuickJS and V8 agree on `Math.sin`, `cos`,
`atan2`, `hypot`, `sqrt`, `pow`, `exp` and `log` to the bit on macOS.

### `cargo build -p parcad-occt` does not build the worker

Without `--features kernel` you get only the host half and the binary is skipped
entirely. Use `tools/build-worker.sh`.

### A Tauri sidecar is declared, staged and installed under three names

`externalBin` in `tauri.conf.json` names `binaries/parcad-occt-worker`. The file
on disk has to be `binaries/parcad-occt-worker-aarch64-apple-darwin` or a
production build fails outright — that part is loud. What is quiet is the third name: the macOS
bundler strips the triple again and writes
`parcad.app/Contents/MacOS/parcad-occt-worker`. Nothing documents that as a
promise, and it is not the same on every target.

So `host::worker_path()` accepts both names, and `tools/build-worker.sh` writes
the staging copy — 28 MB, gitignored — every time it builds. Skip the script and
`tauri build` either fails or, worse on a stale tree, bundles last week's kernel.
Test a bundle from outside the build tree and with the dev environment removed,
or the thing being measured is your shell:

```bash
cp -R target/release/bundle/macos/parcad.app /tmp/ && cd /tmp
env -u PARCAD_OCCT_WORKER PARCAD_HTTP_PORT=4299 /tmp/parcad.app/Contents/MacOS/parcad-app
```

### tauri-build copies the sidecar beside every app it builds

Its build step (`copy_binaries`, tauri-build 2.6.3) deletes
`target/<profile>/parcad-occt-worker` and copies the staged worker there each
time the app's build script runs, and refuses to build when the staged file is
missing. Both halves bit, measured on 2026-09-17:

- **A fresh clone could not build the app at all.** In a new worktree,
  `tools/check.sh` and a plain `cargo build` stopped at parcad-app's build
  script: "resource path `binaries/parcad-occt-worker-aarch64-apple-darwin`
  doesn't exist". Only `tools/build-worker.sh --release` writes that file,
  which means compiling OpenCASCADE before a build documented not to need it.
  CI runs `--fast`, which never builds the app, and a working checkout keeps a
  staged copy from some earlier build, so nothing noticed.
- **Where it did exist, a debug app got the release worker.** After
  `tools/build-worker.sh`, then `--release`, then `cargo build -p parcad-app`,
  `target/debug/parcad-occt-worker` was the release one, and the debug host
  beside it refused it: "this worker was built with the `release` cargo profile
  and the host with `debug`". Every restage also reran the build script and
  recompiled the app.

`app/src-tauri/build.rs` now removes `externalBin` from the configuration
tauri-build reads — a `TAURI_CONFIG` merge patch, kept on top of any the caller
set — when `DEP_TAURI_DEV` is `true`: tauri's `cargo:dev`, a build without
`custom-protocol`, which is every `cargo build`, `cargo test` and `tauri dev`.
Those builds need no staged worker and leave the one beside them alone, and a
restage rebuilds nothing (0.34 s). A production build keeps it: `tauri build`,
and `tauri build --debug` in `tools/test-desktop.sh`, still refuse a missing
sidecar by name and still copy the release worker into their target
directory, which is why test-desktop.sh runs `tools/build-worker.sh` after its
build. A fresh worktree then passed the whole gate:
`tools/check.sh` green in 486 s, with `PARCAD_OCCT_PREBUILT` set and the two
`bun install`s it names, which it now asks for before it builds anything
rather than after the release build. `bun run tauri build` still bundles a
working kernel — the bundled `parcad` measured a 10 mm cube at 1000 mm³
through the sidecar beside it, 18 KB larger than `target/release`'s because
the bundler signs it.

### zsh aborts a command on an unmatched glob

`rm -rf target/release/build/occt-sys-* target/debug/build/occt-sys-*` dies on
the second (already-deleted) glob *before running anything*. Use:

```bash
find target -maxdepth 3 -name "occt-sys-*" -type d -exec rm -rf {} +
```

### The WebAssembly kernel: four ways a build links something else

All met building `web/build-kernel.sh`; each produced a binary, not an
error.

- **A rebuilt OpenCASCADE is not relinked.** `opencascade-sys` asks for
  `static=TK…`, and rustc copies every one of those `.a` files into the crate's
  own rlib when it compiles it. Replace the libraries and cargo sees nothing
  changed: an instrumented `libTKMesh.a` that `strings` showed printing never
  printed, because the worker still linked the copy inside the rlib. The script
  `cargo clean -p opencascade-sys` after every OCCT build. The same holds for a
  native `PARCAD_OCCT_PREBUILT` whose contents change under an unchanged path.
- **ninja keeps objects when a source goes back in time.** Staging with
  `rsync -a` restores upstream mtimes, so a file rewritten to *older* contents
  looks older than its object and is not recompiled. Staging is by checksum with
  fresh mtimes for that reason.
- **setjmp inside a Wasm-EH `try` is invalid wasm.** OCCT defines
  `OCC_CONVERT_SIGNALS` on every non-MSVC build, which puts a `setjmp` in each
  `OCC_CATCH_SIGNALS` block; clang then emitted a function V8 rejects at
  instantiation ("br_table: label arity inconsistent",
  `ShapeUpgrade_ShapeDivide::Perform`) and binaryen could not parse. The define
  only matters once `OSD::SetSignal` installs a handler, which nothing in parcad
  calls, and wasm has no signals; `web/occt-emscripten.cmake` undefines it.
- **`--bin` filters every `-p`.** `cargo build -p a --bin x -p b` builds no
  binary of `b`; name each.

## Geometry

### A planar wall meshed two ways

`re-entrant-loft`'s five side walls are planar B-spline patches. BRepMesh's
deflection control measures the midpoint of each wall's diagonal against the
segment (0.8–2.6 mm in-plane, over the 0.01 mm deflection) and asks to insert
it — a point exactly on the link it would split. Native arm64 clang's Delaunay
insertion takes it and produces no new triangle, so the next pass inserts
nothing and every wall stays two triangles (634 in all). The WebAssembly build
splits the link and refines each wall to 266–450 (2240). Measured with the same
prints in both builds: pass 1 leaves 2 elements natively and 4 in wasm, the
first diverging number. Volume (5250.000), area, size, faces and edges agree
exactly, which is why the case holds the triangle count to 260% rather than
either build's number. Across the whole corpus the WebAssembly build moves only
tessellation-derived numbers — at most 1.7e-4 relative in volume or area, 0.002
mm in size, 0.54% in bed contact and 9.8% in triangles elsewhere — and no face
or edge count; web/README.md has the table.

The request itself was the defect: a planar wall's parametric diagonal
midpoint is off the diagonal in its own plane, which the deflection check read
as deflection. Vendor patch 0003 (see "A curve that starts slowly meshed in
ten times the triangles") samples the foot of the diagonal's own midpoint,
which on a plane is the midpoint, so nothing is asked for: natively the walls
are two triangles each and the part 20. The WebAssembly build should now
agree, since there is no insertion left to round, but it has not been rebuilt
to check: if it still refines, `re-entrant-loft` goes red there, which is the
measurement still owed.

### The `left` and `right` views were mirrored

A boss standing off a part's +Y face drew at column 94 of 128 in the `left` view,
where it belongs at 34. `View::rotation` hands back the three screen axes as
model directions, and for both side views that triple had determinant −1: a
reflection, not a camera. Every other view was right, which is why nothing
noticed — a mirrored picture of a symmetric part is the same picture, and every
part in `examples/` is symmetric about at least one of these planes. One sign on
each of the two, and `every_view_says_which_way_it_looks` now asserts the
determinant. Nothing else about handedness was wrong — the mesh, the measurements
and the exports were never involved, and `Op::Mirror` is a different thing — but
an agent reading a side view of a handed part got it backwards, which is a defect
no amount of looking harder at the render would have caught.

### A section cap with a line through it

An M10 bolt and nut cut on Y and drawn iso showed a one-pixel dark line down
the bolt's axis while the bolt measured one intact piece. Not the tessellation
and not the second body: 138 uncapped samples, every one on column 768 of the
1536-sample buffer, which is the model's x = −y line — and in iso that line is
where the nut's vertical corner edge at (8.5, −8.5) projects *exactly*, as
does any rod whose tessellation puts a vertex at 315°. A sample on an edge two triangles share passed neither triangle's
f32 barycentric test (−5e-8 on both sides), so its ray lost one crossing, and
the parity that decides cut face flipped all the way down. Sixteen more sat on
thread flanks within half a voxel of the plane, where the clip compared the
*rounded* depth and counted a crossing on the wrong side.

The rasteriser now snaps corners to 1/256 px and evaluates edge functions in
integers with the top-left rule, the rule Direct3D and OpenGL rasterisers use
so that a sample on a shared edge belongs to exactly one triangle; the clip
reads the unrounded depth. Parity is only as good as the crossing count, and a
symmetric view puts real edges on the sample grid far more often than "an
exact float coincidence" suggested. `eval/cases/section-m10-bolt-and-nut.json`
holds the rod's core and the nut's section fully capped, and
`a_section_cap_has_no_line_where_an_edge_lies_on_the_sample_grid` holds the rod
alone at four sizes that drew the line before.

### A part can be five bodies and pass every check

`examples/extrusion-2020.js` was in the corpus for months as one watertight part
with the right volume and 37 edges, and it was five bars: its four T-slot
chambers overlap at the corners, which severs the core from each corner block.
Nothing measured caught it because nothing measured *counted* — watertightness is
per edge, volume adds, and the face count of five closed pieces is a plausible
number. `bodies` in `MeshStats` (connected components of the mesh, joined by
vertex position) is the number that does, and `stands_on` found it first by way
of the end face being five patches. Both are recorded for every case now.

### A solid can cross itself, or face inward, and pass every check

`BRepCheck_Analyzer` judges each face against its own boundary; it does not ask
whether two faces of a solid run through each other, or which way the solid
faces. The mesh backstops do not either: a self-crossing surface still closes,
and an inside-out mesh encloses the same negative volume as its inside-out
solid, so the two volumes agree. Measured on shapes that built and passed all
of it: a sweep whose last leg crossed its first (the overlap counted twice in
its volume); a star lofted to itself three corners on (volume −3382 mm³); a
tray's bottom edges chamfered deeper than its walls, the chamfer faces running
through the pocket; rounds on either side of a 1 mm gap crossing each other;
a 0.4 mm blend whose corner patch folds over; and, at integration, a walled
smooth loft that probed a point 300 mm outside it as material.

`BOPAlgo_CheckerSI` finds the crossings and a point classified against each
shell finds the orientation; docs/VALIDITY_CHECKS.md has where each runs, what
each costs, and the fuzz sets. The first thing to suspect about a new
construction that turns surfaces into a solid is both of these, not closure.

### A fillet that fails can still change the shape it was given

`BRepFilletAPI_MakeFillet` widens tolerances on the vertices and edges it is
handed, in place, and a `TopoDS_Shape` is a handle: the input, every clone of
it, every earlier probe and any cached subtree sharing those vertices see the
change. Measured on two crossing cylinders: after a 1.5 mm blend that built and
was refused, a vertex of the untouched union sat at 42 mm tolerance, and the
self-intersection check then read 130 vertex contacts on a sound 1.13 mm blend.
Treatments now build on a topology copy (vendor/opencascade PARCAD-CHANGES.md).
Booleans alter their arguments too, but by at most 4.7e-7 mm over the corpus.

### A shell of anything treated or combined was refused, blaming the author

`BRepOffsetAPI_MakeThickSolid` hands back the inward offset of a filleted,
chamfered or combined solid as a bare closed shell, not a solid; subtracting a
shell removes nothing, and the part was refused as "the operations cancelled
all the material away". 37 of 80 fuzzed shells hit it — every one a plain,
buildable part. The cavity is now closed into a solid and turned outward, and
the result is held to part volume minus cavity volume;
`shell-filleted-box` and `shell-of-a-union` hold it to closed forms.

### A correct solid can mesh as a closed fragment of itself

A model building a unicorn over MCP (2026-09-14) reported that "unions lost the
body" while the report said built and watertight. Reproduced, the union is not
the defect:

```js
union(sphere(10), sphere(10).rotate("x", 90))
```

The fuse returns **4188.79 mm³ exactly** (`BRepGProp`), already one face, and
`BRepCheck` calls it valid. `BRepMesh` then triangulated that face as 198
triangles enclosing **727.70 mm³**, a 10 × 20 × 20 closed fragment: watertight,
one body. Every number the report shows is read off the mesh, as are the
preview and the STL, so all of them described the sliver; only a STEP export
was right.

The common factor is an operand unioned with a rotated or mirrored copy of
itself, so both share one curved surface with different parameterisations —
which a figurine does every time a copy lands on the original. It is three
defects, each in a representation rather than the geometry:

| union with its copy | before | cause | after |
|---|---|---|---|
| `sphere(10)` turned 90° about X, or also moved | closed fragment, 727.70 mm³ | copy's seam left on the face as an INTERNAL wire; BRepMesh meshes only the region it cuts off | 4188.79 mm³ B-rep, mesh 4182.71 mm³ — plain `sphere(10)`'s |
| `sphere(10)` turned 37° or 89° about X | open mesh (4 / 2 edges) | same | same |
| `cylinder(5,20)` turned 45° about Z | open mesh, B-rep 523.60 mm³ | `Shape_drop_unused_seam_pcurves` dropped a seam pcurve the other half of the split side still used — pcurves are per surface, not per face | 1570.80 mm³, 3 faces, mesh = plain cylinder's |
| `torus(10,2)` turned 30° about Z | "a mesh with no vertices" | `UnifySameDomain` welds the halves into one face with no wires; BRepMesh skips it | 789.57 mm³ B-rep, mesh 786.57 |

The fixes are in the vendored crates (see their PARCAD-CHANGES.md): after every
unify, `parcad_tidy_faces` drops INTERNAL edges and rebuilds a wireless face
with its natural bounds, and the seam pass keeps a seam another face on the
same surface borders. `AllowInternalEdges(false)` does not strip existing
internal edges; it only stops the unifier making new ones. Measured over
sphere, cylinder, torus and cone, each unioned with itself turned 30/45/89/90/120/180°
about X, Y and Z and mirrored in each plane: all 84 build and pass both
backstops. In the corpus only `tangent-blend` moved — two internal imprint
lines on its side walls gone, 50 edges to 48, volume unchanged.
`ShapeFix_Shape` also rebounds the torus but leaves the sphere's internal wire;
`ShapeUpgrade_ShapeDivideClosed` makes the sphere BRepCheck-invalid; a finer
angular deflection meshes the same fragment.

The worker keeps three backstops behind this: a mesh that does not close; a
face the mesher left untriangulated; and a closed mesh whose enclosed volume
differs from the solid's by more than `2 · area · deflection`, the most a
tessellation within that deflection can account for. Each names the fix for a
cause not yet repaired: overlap the operands by 0.01 mm instead of letting a
copy coincide. `eval/cases/coincident-*-union*.json` hold the four rows down.

Two guards were tried first and taken out because the defect never reached
them: a union volume floor (result ≥ larger operand) and a volume check across
`UnifySameDomain`. Both read the exact B-rep, which was never wrong.

### A closed chain of 2D curves concatenates starting at its last piece

A part of 2026-09-18 put sixteen `sphere(1)` beads on a `sphere(8)` at a bolt
circle of 7.4 mm and would not build: 116 of its mesh edges bordered one face.
It reads like the section above, and it is not — those are an operand fused
with a *copy of itself*. Here the boolean was exact every time:

| stage | faces | volume | BRepCheck |
|---|---|---|---|
| `sphere(8)` | 1 | 2144.6606 | valid |
| `sphere(1)` | 1 | 4.1888 | valid |
| after `BRepAlgoAPI_Fuse` | 3 | **2145.1397** | valid |
| after `clean()` | 2 | **2140.7807** | `BRepCheck_NotConnected`; the bead's face unorientable |

When a bead's kept cap straddles the bead's own seam (u = 0), the fuse returns
the cap as two faces sharing that seam, and the intersection circle as two arcs
meeting where the seam crosses it. The unify's face pass welds the cap into one
face and drops the seam, correctly. That leaves the arcs meeting at vertices of
degree two, so the edge pass merges them into one closed circle — and the new
edge's pcurves sit **1.233 mm off its 3D curve at every parameter**, on both
faces: the chord of 1.673 rad, the first arc's span. `ShapeFix_Face` then
rebuilds the bead's face around a degenerate edge at its pole, and the bead is
gone.

The shift is `Geom2dConvert::ConcatC1`. It joins each piece to the group so far
with `Geom2dConvert_CompCurveToBSplineCurve::Add(curve, tolerance)`, whose
`After` argument defaults to `false`. `Add` reads it only when the new piece
meets both ends of the group — a chain that closes — and there the default
prepends the last piece, so the concatenation starts one arc late.
`UnionPCurves` reparametrises that curve against a 3D circle that starts at the
first arc. The 3D twin, `GeomConvert::ConcatC1`, already passes `true` at the
same step; `vendor/occt-sys/patches/0004-concat-closed-chain-appends.patch`
makes the 2D copy do the same. Called directly, `ConcatC1` returns identical
poles before and after for every open chain tried, so nothing else moves.

What each suspect was cleared by, since each looked right at some point:

- the mesher and the backstop — the B-rep was invalid and short of volume
  before anything meshed it;
- ParCAD's seam-pcurve pre-pass and `parcad_tidy_faces` — skipping either
  changes nothing;
- the loosened merge tolerances — 1e-4 through 1e-9 break identically;
- the order of the unify's two passes. Edges before faces builds this part,
  because the edge pass then runs while the seam still splits the circle — but
  it leaves unmerged every edge a face merge would have exposed, and five corpus
  cases gained edges. A third edge pass restores them and breaks the bead
  again. Ordering only moves the defect;
- `UnionPCurves`' projection fallback and `ShapeFix_Face` — each is correct
  when handed a correctly parametrised edge.

The window is narrow and closed form, which is why this arrived as "unions are
broken" rather than as a seam question. The bead's kept cap has half-angle
arccos(0.557) = 56.2°, so its seam cuts the circle only below that; below 5.96°
the big sphere's seam cuts it too, the arcs meet a third edge, and they never
merge. Swept a degree at a time, one bead failed from 6° to 52°. `polar(4, …)`
puts every bead on ±X or ±Y and builds; `polar(16, …)` does not.
`eval/cases/beads-on-a-sphere.json` holds it.

**The failures were the loud end of it.** From 53° to 56° the first arc is short
enough that `ShapeFix_Face` lets the shift through: the part builds, BRepCheck
passes and the volume is right, while the merged edge's pcurves sit 0.759,
0.641, 0.481 and 0.186 mm off its 3D circle. No backstop reads that and no corpus
number moves with it; it is measured with the edge's own curves, the surface at
its pcurve against its 3D curve.

### A seam cut the rim of a bead in two

OpenCASCADE gives every closed surface a seam — a sphere's runs pole to pole
along +X — and a seam has to end on whatever boundary it crosses. Fuse a ball
onto a sphere on +X and the sphere's seam ends on the ball's rim, putting two
vertices on it: the rim is two kernel edges. Nothing is wrong with the solid.
But ParCAD already hid the seam itself, so what a person saw was one bead in a
ring of fifteen with its rim halved, and a selector over the rims matched 16,
so `.expect({ count: 15 })` failed on a part with fifteen beads.

Moving the seam only moves the split: the big sphere turned 12° about Z put it
between the beads and onto the fillets' circles instead, 21 faces and 27
edges against 19 and 20. The representation needs its seam somewhere, so the
fix is in what ParCAD counts, not in the solid. `Shape::logical_edges` groups
the kernel's edges into the part's: seams, splits between faces of one surface
and degenerate poles are no edge, and at a vertex that only such an edge also
reaches, two pieces of one curve are one edge. The viewer and every selector
read the same grouping, so they cannot disagree; a treatment still receives
every kernel piece. "One curve" is exact — the same `Geom_Curve`, or equal
lines or circles — and two different curves meeting there stay apart.

What moved: the drawn-curve count of 16 corpus cases, each join checked to be
two pieces of one circle at a seam or split (pipe and tube rims, a cylinder
section drawn as two arcs, arcs across +X); one recorded `expect({ count: 2 })`
that had been counting a tube rim's two halves, now 1; and no volume, face,
kernel edge or validity anywhere, and every seed part still builds.

### Delabella, OCCT's other triangulator, meshes an extruded spline open

`IMeshTools_Parameters::MeshAlgo`, or the `CSF_MeshAlgo=delabella` environment
variable the default factory reads, swaps OpenCASCADE's Watson triangulator for
Delabella. It is faster — on ParCAD web's planter, 199k triangles over 37
revolved bumps, the mesher went 1.27 s to 0.94 s and the whole part 3.29 to
2.86 (M4 Max, release); in WebAssembly it saved 6% of the mesh time — and it
returns meshes that do not close. Two lines are enough:

```js
return extrude([[-10, 0], [10, 0], { bezier: [[0, 20]] }], 3);
```

82 of that shape's 240 mesh edges border one face instead of two, which the
worker's first backstop refuses; a `{ spline: … }` section reads 98 of 288. The
same shapes mesh watertight under the default. The failure follows the *surface*,
not curves in general: a box, a cylinder, a sphere, an `extrude` whose curve is a
circular arc, a `revolve` with a spline section, and a `loft` between those very
Bézier outlines all pass. What fails is a straight extrusion of a B-spline, and
the planar caps that curve bounds. Two corpus cases hold it down —
`parabola-bezier` and `curve-edges-by-kind` — and two more drift past tolerance
under it: `probe-port-meets-gallery` reads a 5 mm wall as 5.003, and
`thread-m8-3-turns` gains 6.2% triangles.

Delabella is not OCCT's default and its factory chooses a different algorithm per
surface type, so this is a less-travelled path; measured on OCCT 8.0, 2026-09-16,
and not reported upstream (their tracker's only Delabella item is an unrelated
pointer-arithmetic fix from February 2026). The symptom is all that has been
observed — free edges counted by our own check — not a dump of which face loses
its triangles. `MinSize` was measured at the same time and is not worth having
either: at 0.05 mm it removes 1% of the triangles and no time, because the count
is set by real curvature, not by slivers.

### A helix cut through its own cylinder opened past two turns *(fixed before it was diagnosed)*

A groove swept along a helix and cut from a cylinder on the same axis, at the
helix's radius, was recorded in the helix commit (`32feba99`) as building and
closing at one and two turns and not at three:

| `cylinder(3, 10).cut(pipe({ helix: { radius: 3, pitch: 2, turns } }, 1))` | at `32feba99` | on main since `a5f1ccda` |
|---|---|---|
| 1 or 2 turns | watertight | watertight, 275.82 / 268.90 mm³ |
| 3, 4 or 12 turns | 8 mesh edges border one face; refused | watertight; 3 turns 261.98 mm³ = closed form |
| a V groove, 8 turns | 261 open edges (117 in the original script); refused | watertight |
| a V ridge unioned onto a core, 8 turns | 1361 open edges (1043 in the original); refused | watertight |

**The boolean was never wrong, and neither was the mesher.** The defect was
parcad's own `Shape_drop_unused_seam_pcurves`, the pre-pass `clean()` runs
before `UnifySameDomain`. A helical groove splits the cylinder's side into
strips — one more per turn — that all lie on one `Geom_CylindricalSurface`,
and each strip's wire uses its piece of the seam line once. The pass read "used
once" as a stale seam and dropped one of the edge's two pcurves; but pcurves
are stored per surface, not per face, and the neighbouring strip was using the
one it dropped. Measured stage by stage in a C++ probe mirroring the pipeline
at 3 turns:

| stage | B-rep volume | BRepCheck | open mesh edges |
|---|---|---|---|
| `BRepAlgoAPI_Cut` | 261.9828 mm³ (closed form 261.982900) | valid | 0 |
| seam pass, unguarded | 205.7516 | `UnorientableShape` ×2 | 8 |
| seam pass, guarded | 261.9828, 0 pcurves dropped | valid | 0 |

Serial or parallel, one `Build()` or two, the same. The fix is the guard
`a5f1ccda` added for a different part — a cylinder unioned with its own rotated
copy, GOTCHAS "A correct solid can mesh as a closed fragment of itself" — which
keeps a seam another face on the same surface borders. The helix branch was cut
from main before that commit and measured there; nobody re-measured after the
merge. Confirmed in the real pipeline: the helix commit's own worker opens by
exactly the recorded counts, and the same tree with only that 15-line guard
applied closes every row with main's numbers. `eval/cases/helical-groove.json`
holds the 3-turn cut to its closed form.

At one and two turns the unguarded pass dropped pcurves too; in the pipeline
the result still meshed and exported right (STEP 268.903 mm³ at two turns),
which is why the table's first row passed.

### Threads: which construction measures right

`threadedRod` and `threadedHole` build an ISO 68-1 basic-profile thread as
`Op::Thread`. What was measured before choosing, on an M8 × 1.25 and then on
every coarse size M2 to M20 at 1 to 20 turns, both hands, against the slab
closed form `V = π r1² L + 2π L / P · ∫ r w(r) dr`:

| construction | from | result |
|---|---|---|
| tooth in the axial plane swept (MakePipeShell, Frenet) along a **one-edge** helix; core ∪ tooth; ends cut by two boxes | parcad's `sweep` | valid and closed; 206 of 220 within 1e-5, the rest 1–3e-5 — the fitted spine |
| the same along a helix of **one edge per turn** | FreeCAD `makeLongHelix` + PartDesign Hole | 220 of 220 within 1e-5 (the rods about 1e-6, the tooth alone 8e-7 of Pappus); bolt and nut pairs 36 of 36 |
| the same, end faces sewn on (FreeCAD's exact route) rather than `MakeSolid` | FreeCAD | the rods the same; the tooth alone 4e-9, below anything the boolean after it keeps, so parcad keeps `MakeSolid` |
| four ruled faces between helices, two planar caps, sewn | cq_warehouse `Thread` | rods right, but the tooth alone read +25% / −12% of Pappus while `BRepCheck` passed it: rejected |
| `ThruSections` (ruled) between two closed wires on coaxial cylinders | OCCT MakeBottle tutorial | invalid solids of wrong, some negative, volume at 3, 8 and 20 turns; the tutorial's two-turn ellipse is not a 60° thread, and it never fuses its thread either |
| one 11-section loft per turn, fused | bd_warehouse `Thread` | valid, closed, and short everywhere — the tooth 1.1e-4, the rod 8e-5: a loft approximates the helicoid |

And two ways to finish the chosen construction that return **valid, closed,
wrong** solids with no warning beyond `BOPAlgo_AlertFaceBuilderUnusedEdges`:

| finish | result |
|---|---|
| core **shorter** than the swept tooth, so the tooth overhangs the core's ends | 94 of 132 rods 16–100% short, some empty |
| ends squared by a **common with a cylinder** rather than by cutting boxes | fails the same way on the overhanging form; on the chosen form with a one-edge spine, 5 of 132 fail once the cylinders' seams are turned 37°. A box has no seam |

So `build_thread` in `backend.rs` sweeps along one edge per turn, gives the core
exactly the swept height, and cuts two boxes; and because the wrong answers
above were all valid and closed, it measures every thread it builds against the
closed form and refuses one more than 2e-5 off. Two things follow for authors:
the tooth crosses +X at z = 0 of the thread's own frame, so a rod and a hole
mate only a whole number of pitches apart (or turned by 360° × offset / pitch)
— `between_bodies` reads a pair out of phase as interfering — and a clearance
`c` on each part is `c` across the flanks, shortened by the lead angle to
`c (2/√3) / √(4/3 + (P / 2πr)²)`.

### The volume integral misreads a wavy B-spline wall

`Shape::signed_volume` — `BRepGProp::VolumeProperties` with the adaptive
`Eps` — is the reference the mesh backstop compares every mesh with, and on a
wall extruded from a wiggly B-spline it is the number that is wrong. Measured
on a 120-point circle of radius 20 with ±0.15 mm of noise on every point,
extruded 10 (the true disc is 12566 mm³), against the area the curve's own
dense samples enclose by Green's theorem:

| curve through the points | samples × 10 | mesh at 0.01 mm | B-rep, eps 1e-7 | B-rep, eps 1e-10 |
|---|---|---|---|---|
| `{ fit }` at 0.2 mm, 35 poles | 12568.9 | 12564.1 | 12666.6 | within the allowance |
| `{ spline }` (interpolating), 123 poles | — | 12571.3 | 12828.3 | 11947.7 |

The mesh sits where a chord tessellation should, 0.04 % under; the integral
is 0.8 % high on the fit and 2 % high on the interpolant, and *5 % low* on
the same interpolant when asked for more precision — not converging, moving.
Nor is waviness needed: a 36-point cam fitted to 19 poles at 0.05 mm and
extruded 8 read 13045 mm³ against a mesh of 12662 and samples of 12666, and
the field suite's first model spent eleven refused calls on it. The surface
is the culprit, not the curve — `MakePrism` sweeps a B-spline edge into a
`Geom_SurfaceOfLinearExtrusion`, and the same cam as a ruled loft between the
section at its two heights (a `Geom_BSplineSurface` of the same shape) reads
12663.5. So `Op::Extrude` now sweeps every curved outline as that ruled loft
and keeps `MakePrism` for polygons; the three curved extrusions in the corpus
kept their volumes to the recorded digits and mesh in a fifth of the
triangles.
`BRepGProp::SurfaceProperties` has the same trouble one dimension down: on
the planar face a lamp section's fitted curve bounds (131 poles) it read
8488 mm² where the curve's samples enclose 8835, and the inset guard built
on it refused a good inset for "growing". Faces from arcs, planes and smooth
fits of few poles integrate to the last digit, which is why the corpus never
saw it; the backstop's refusal on such a part is a false alarm that reads as
a missing surface, and its message now says so. Not fixed at the integral:
the mesh is what every reported number is read from already, so the volume
defect reaches only that guard, extrusions no longer make the surface it
misreads, and the inset guard measures both its areas by Green's theorem over
dense samples of the wires instead. A sweep along a straight path still can. Fit the curve at
a looser tolerance, or smooth the points, and the integral settles. `eps`
stays 1e-7.

### A curve that starts slowly meshed in ten times the triangles *(fixed)*

The mesher judged a face in its parameters, so how fast a curve ran through
its own parameter showed up in the triangle count even when the shape was the
same. One arc of radius 20 over 2.5 rad, extruded 10: drawn with `{ curve }`
in its angle, 240 triangles; with angle `t²/2.5`, so it starts at rest, 1030.
The involute is the case that matters: its speed in the roll angle is `rb·t`,
zero on the base circle — intrinsic, because its curvature is infinite there
and a polynomial can only follow that by stopping — so a certified 20-tooth
gear meshed in 20300 triangles against 1916 for a `fit` through the same
flanks, and its mesh took 182 ms against 117.

Two assumptions in BRepMesh, both about B-spline faces, and
`vendor/occt-sys/patches/0003-mesh-deflection-at-the-foot.patch` changes both:

- **The deflection check sampled a link at its parametric middle.** On the
  ruled flank wall a link running up the wall has its parametric middle
  `rb·dt²/8` along the wall from its own middle — 0.014 mm on one knot span of
  a module 2 gear, over the 0.01 mm deflection — so every diagonal "failed",
  was split, and the new diagonals failed the same way: eight passes and 450
  triangles a flank. The sample is now the foot of the perpendicular from the
  link's (or triangle's) own middle. Measured over the whole corpus that is
  also the more *correct* check: the parametric middle had let parts mesh
  outside 0.01 mm (the helical groove to 0.011, a helical spring to 0.017),
  and the parts that gained triangles are those, now all nearer their
  surfaces and seven of them inside the deflection.
- **A knot where the normal cannot be computed added a row of nodes.** The
  flank's zero-speed edge is such a place, and it sits on the face's
  boundary, where no node is ever inserted; the row it added at v = 0.5
  doubled every flank. Only knots strictly inside a face count now.

The node inserted for a failing element is the sample itself — the surface
point that failed. Inserting it at the parametric middle with the sample at
the foot was tried, and left every twisted or helical part measured outside
its deflection (`untriangle-v3` 0.0096 → 0.0166 mm worst; the split no
longer removed the deviation it was made for, and the passes ran out).

### A twisted wall's straight edge was one link *(fixed)*

`untriangle-v3` built on macOS and Linux arm64 and was refused on Linux
x86_64 with two of 38616 mesh edges bordering one face. Not the compiler.
BRepMesh discretises an edge by its own curve, so a straight ruling of a
twisted wall stays one boundary link however far the face's normal turns
along it, while every interior link is held to `AngleInterior` (1 rad). The
triangle standing on the ruling then always has a side to the far corner
spanning the whole turn; the interior splits that side at its middle each
pass, the new node is the next apex, half as near the ruling, and only the
11-pass cap ends it. On x86_64 the two walls sharing a ruling ended their
chains 0.0008 mm from it, and the 0.001 mm weld fused the pair into one
vertex with four triangles on each of its edges; arm64's rounding stopped a
pass short and they stayed apart. A MinSize floor against frontier
links was tried and measured inert: the chain runs through the angular
check, which never asks it. `vendor/occt-sys/patches/0005-split-edges-where-
the-normal-turns.patch` splits a boundary segment where the normal turns
more than `AngleInterior`, a node both faces share by construction, and a
Delaunay triangle cannot then reach from an apex near the ruling past its
neighbours on it. `crates/parcad-occt/examples/mesh_deviation.rs` is how
"the mesh got coarser" and "the mesh got worse" were told apart.

After: that arc 252 triangles, the certified gear 1996 in 120 ms (the fit,
1916 in 116), the seeded gear pair 36428 → 5452 and 415 → 302 ms. The whole
corpus, read back from STEP and meshed both ways, lost 12.6 % of its
triangles and 11 % of its mesh time, and no part's worst sampled distance
from its surface grew. `involute-gear` holds the count. Neither the knot
scale, double knots, nor the extrusion was the cause, as was once checked.

### A sideways inset of a leaning wall is thinner than the inset

Stepping a section inward by `t` in its own plane makes a wall `t · cos φ`
thick, square to a surface leaning `φ` from vertical. The fitted lamp shade
built as outer sections minus sections stepped in by 1.6 mm measured 1.211 mm
at its thinnest in `measure_wall_thickness` (kind `wall`, not a rim artifact),
and a horizontal inset of one sloped section measured 1.387 mm. `loft(...,
{ wall })` steps by `t / cos φ` from the built outside, so its `loft_wall_mm`
is the wall square to the surface. It now steps along the outside's surface
normal rather than sideways, with the height solved so floors and rims stay
level: the sideways step needed `t / cos φ`, which refused anything within
14° of flat, and a bowl or a dome was impossible to wall.

### `BRepLib::OrientClosedSolid` can reverse a solid that was right

It classifies the point at infinity by one line along a face's normal and
trusts the transition at its farthest crossing. A walled pleated shade
(`walled-twisted-pleats`: 288 points, 21 sections, a 35° twist) came out of
the sewing facing the right way; the classifier answered IN, the solid was
reversed, and the part was all of space except the shade — while
`mass_properties` reported `volume.abs()` and every check passed. The same
script with 25 sections and a 70° twist got OUT. Nothing about the geometry
is wrong; the line through dozens of walls 1.2 mm apart on large B-spline
bands misses a crossing. The skinner states which way is out and checks the
face, so a skinned loft is never classified. Everything else that turns
surfaces into a solid is classified per shell (`facing_outward`, a point
outside the box against each shell on its own — the same kind of ray, so it
can misfire the same way), and behind all of it the mesh backstop in
`serve.rs` checks every closed mesh shell's winding against its nesting,
which a turned solid cannot pass. Built by the fitter that came after,
the same shade faces the right way even under the old classifier, and no
lighter variant of it (48 tried) flips; the case is kept for the backstop,
which turns any recurrence red.

### Uniform knots over uneven points leave spans empty

A least-squares fit on uniform knots needs every span to hold points
(Schoenberg–Whitney). A pleated section sampled at equal steps along its
base star, then pushed out and in, had chord steps from 0.76 to 3.31 mm: on
256 uniform spans its fit plateaued at 0.15 mm, and on 511 — nearly one span
per point — it was still 0.163 mm off, because the long chords had spans with
no point in them and the matrix was only nominally invertible. Knots placed
by the parameters (same points per span) fit the same section to 0.0007 mm
on 508 spans. `PeriodicFit` refuses a factorisation whose pivots span more
than twelve orders of magnitude.

That section's 0.15 mm plateau is its own: at `z = 0` near (67, 27) its
points zigzag, turning ±1.4 mm every 0.77 mm, a staircase the pleat growth
left behind. Held to 0.05 mm the outside must follow it, and a 0.8 mm wall's
inside loops there at every refinement — refused, naming the place. At
0.25 mm the fit smooths it and the wall holds.

### `Edge::fit`'s closed seam is only G1, and can loop

`AppParCurves_TangencyPoint` fixes the *direction* of the tangent at each end
of the fit, not its size: `AppParCurves_LeastSquare` solves the two end
magnitudes (`lambda1`, `lambda2`) freely along with the poles. So the closed
`{ fit }` a section resolves to meets itself with one tangent direction and
two speeds, and where the least squares wants the curve slow — the inside of a
lamp at a star tip, where the offset turns with a radius of 0.4 mm — one
magnitude goes to nearly zero and the curve cusps and loops by 0.01 mm at its
start, at every span count up to the interpolation limit. The walled loft
found it and does not use `Edge::fit`: its sections are fitted periodic
(`parcad_core::skin::PeriodicFit`), with no seam at all.

A closed `{ fit }` section in an extrude, a revolve or anywhere else no longer
does either: `section_wire` fits it as a one-section skinned loft
(`skinned::fit_closed`) — parameters corrected to follow the curve, knots
following the parameters, the fewest spans that hold, C2 through where the
points start — and measures the deviation again on the edge OCCT holds. The
six-lobed outline of `fit-six-lobes` (180 points) measured:

| tolerance | `Edge::fit` | periodic fit |
|---|---|---|
| 0.6 | 35 poles, 0.594 mm | 22 poles, 0.436 mm |
| 0.1 | 131 poles, 0.042 mm | 27 poles, 0.067 mm |
| 0.05 | 131 poles, 0.042 mm | 49 poles, 0.044 mm |
| 0.02 | refused | 57 poles, 0.017 mm |

`Edge::fit` still makes the open fits between two corners, whose ends are
the corners and have no seam. Both keep a fit to four fewer free poles than
points: at the limit the loft's fitter used to allow, one fewer, a circle
with 0.3 mm of noise "held" 0.01 mm on 122 poles for 120 points, which is
interpolation.

### One large B-spline face meshes far slower than the same surface in bands

The fitted lamp as one smooth outer face and one inner (4 faces) spent 102 s
in the loft's bounding-box tessellation at 0.01 mm; the same two surfaces cut
into a face per stretch between its 15 sections (30 faces) took 18.7 s, and
the mesh after it 1 s. `ThruSections`' single smooth face in the older lamp
took 166 s. Splitting further, in `u` as well, did not help measurably (16 to
19 s under load) on that surface's few spans; on 512 it does (below). So a skinned loft is always banded at its sections, and the
edges between bands are `dihedral: "smooth"`.

Bands cut from one `Geom_BSplineSurface` do not stay bands, though:
`ShapeUpgrade_UnifySameDomain` counts two faces on the same surface *handle*
as one domain, so the cleanup after any boolean welded them back into one
face per skin. A lamp of two ruled 41-section lofts and a cut came out with 4
faces and took 78 s (35 s of it tessellating those faces) where
`ThruSections` had taken 17.6 s with 82. Each band is now its own segment of
the surface (`Geom_BSplineSurface::CheckAndSegment`, exact knot insertion),
which unify leaves apart; a whole-surface copy per band would too, but
segments are smaller and the same lamp built in 6.3 s against 10.6 s with
copies. The segments change only where the mesher puts triangles — it splits
a face at its own knots — so the fitted-frustum case meshes in 20422
triangles instead of 14842, within the same 0.01 mm.

The mesher also ran one face at a time: the binding built
`BRepMesh_IncrementalMesh` with `isInParallel` off, and a sample of the worker
showed OCCT's thread pool idle while it meshed eighty independent bands. It is
on now (`Mesher::new`, `Shape::write_stl`); every corpus case other than the
two skinned ones meshes to the identical triangle count, volume and area, and
the corpus's summed mesh-and-build wall time fell from 47 s to 29 s.
### A fitted curve is wider than its points

A `{ fit }` section's box was taken as its points' box plus its tolerance, and
a loft was refused when the solid reached past it. But a smooth curve peaks
between its samples: the star r = 30 + 12 cos 5a sampled at 60 points reaches
y = ±39.944 at the points and ±40.201 on the curve, and every loft of it was
refused as "bulging 0.20–0.27 mm outside its sections' own extent" — shifted
pairings and the unshifted one alike — while the fit was right. The check now
compares the solid with the curves as fitted (`BSpline::extent`, closed form
on each span of a cubic; a knot is a station, since a turn exactly on one
solves to the span's end). The graph's `framing_bounds` still use points plus
tolerance, since no curve exists before the kernel runs; the reported bounds
are measured.

The same lofts logged a ruled facet sag of 2.3–2.7 mm where two sections make
the ruled loft the smooth one. `Surface::distance_near` descended only from
the nearest point of a coarse grid, and on a wall twisted seven points round
another sheet passes nearer a grid point than the sheet the point is on; it
now also descends from the point it is given (`fitted-loft-shifted-star`).

### The volume integral misreads a thickened pleat

A pleated shade thickened to 1.4 mm read 160 731 mm³ through
`Shape::signed_volume` (adaptive, `eps` 1e-7), 153 800 through the fixed-order
default, and its mesh 165 584; Gauss–Kronrod over every knot span
(`BRepGProp::VolumePropertiesGK`, spans on) converged on 165 729.6 — the
mesh's number less its chord — but took 25 s at `eps` 1e-3, still 1.8 % off,
and 538 s at 1e-7. A deeper pleat read 15 % low.

A closed form settles which number is wrong. A surface lofted through one
pleated curve at three heights is a cylinder over the curve, with no Gaussian
curvature, so a wall centred on it and closed along its normals holds exactly
t × L × h, L the curve's length (half the surface's free edge length). For 24
pleats 4 mm deep round 60 mm, 200 mm tall, 1.4 mm thick
(`a_thickened_pleat_reports_the_volume_its_closed_form_gives` in
`surfaces.rs`, the `thickened-pleat-cylinder` case):

| reading | mm³ | off |
|---|---|---|
| t × L × h | 155 888.9 | — |
| Gauss–Kronrod span by span, `eps` 1e-4 | 155 896.8 | +0.005 % |
| the mesh at 0.01 mm (what the part reports) | 155 981.4 | +0.06 %, inside its chord bound |
| `signed_volume`, adaptive whole-face | 556 182.5 | +257 % |
| `BRepGProp::VolumeProperties`, fixed order | −170 703 | wrong sign |

So the whole-face integral cannot arbitrate a mesh on these walls, and the
mesh backstop, which compares the two, refused good shades. It now asks,
before refusing, the question the volume stands in for: does every face's
triangulation cover the face? In each face's parameter plane the triangles'
area must equal the area the face's own mesh boundary — the polygon every
edge was discretised into — encloses, to rounding (`Shape::uncovered_faces`);
a mesh of part of a face falls short. The nodes of a triangulation lie on
their surface, so a mesh that covers every face bounds the solid to within
its chord, and the part's reported volume is always the mesh's, never the
integral's. Only when some face fails does the old verdict stand. Anything
else that needs a B-spline solid's volume integrates span by span
(`Shape::volume_by_spans`); `stitchSurfaces` does.

### A skinned surface in whole bands meshes in minutes and unifies in seconds per face

A shade skinned on 512 knot spans, cut only at its sections, took 470 s to
mesh at 0.01 mm; in pieces of 32 spans along u, 91 s under load, and 23 s with
the mesher's faces on every core (`BRepMesh_IncrementalMesh`'s parallel flag,
which meshes the same triangles). The pieces must be *segments* of the
surface, not faces trimmed from one: `ShapeUpgrade_UnifySameDomain` asks
`GeomLib_IsPlanarSurface` of every neighbouring pair, which samples the face's
whole underlying surface — `8 + 3 × intervals` points each way — and an offset
of a 512-span surface is that for every one of 672 faces: 43 s of a boolean
that changed nothing, 0.5 s once each face had its own segment. The same
unify welds faces that *share* one surface back into one face.

### A thickened fold meshed open

A deeply pleated shade skinned on 1024 spans, its fold tips rounded to 1.5 mm
and thickened 1.4 mm, built, measured 1.4 mm everywhere and passed the
kernel's checker, and its mesh had 2094 open edges among 18.6 million. The
tips were rounded on the script's *points*; the curve fitted through them
turns tighter between points than the points do. On a closed form — 24
pleats round 60 mm, the fitted curve's tightest turn found by dense sampling
— 6 mm pleats turn at 0.740 mm (0.857 mm on the function sampled), so half of
a 1.4 mm wall leaves the inside skin turning at 0.04 mm. Its surface moves at
5 % of the speed of the surface it was offset from, and `BRepMesh` left holes
inside five of its faces, 0.4 to 5 % of each face's parameter area (19 %
with a finer angular deflection). Measured by pleat depth:

| pleats | tightest turn | offset × curvature | mesh |
|---|---|---|---|
| 5 mm | 0.912 mm | 0.77 | closed |
| 5.5 mm | 0.821 mm | 0.85 | closed |
| 5.8 mm | 0.770 mm | 0.91 | open in two faces |
| 6 mm | 0.740 mm | 0.95 | open in five faces |

Two faults let it through. `thicken`'s fold check sampled a 6 × 6 grid over
each face, and a face of 32 knot spans hides every pleat tip between those
points; and it refused only past 0.98. It now searches each face from four
points in every continuous stretch of its surface each way, climbs the
extremes to 1e-9 of the face's parameters (`Shape::bend_extremes`), and
refuses past 0.88, naming the radius, where it is, and the rounding that
would pass.

### `thicken` measures its wall on the part's own mesh

Reading the wall at 9360 points of a pleated shade by `Extrema_ExtPS` — a
sample grid rebuilt per point on each nearby offset B-spline face — took
12.5 s native and 18 s under WebAssembly, three quarters of the build. The
thickened solid is now meshed first, with the same `Mesher` the part's report
uses, so `NearestBoundary` seeds its Newton steps from triangles; and every
face the later cut leaves alone keeps that triangulation, which the final
`BRepMesh_IncrementalMesh` reuses rather than repeats. The readings are the
same exact distances (1.40000 to 1.40000 mm). What is left is the mesher
itself: 546 offset faces at 0.01 mm are about 28 s single-threaded in a tab
(3 s on every native core), and no parameter that keeps the deflection
bound makes that smaller.

Bounding them was the other half: `AddOptimal` on an offset face runs a
particle-swarm search per coordinate, 4.3 s for a shade's tag extent. Only the
faces whose enclosing box reaches past what the tag's mesh nodes already
reach are optimised now, which gives the identical box (checked bit for bit
on the corpus's tagged parts) from a few dozen faces.

### A thickened surface's rim leans

`thicken` closes the wall at a free edge with a face along the surface's
normals there, so the rim of a shade whose wall leans is not flat: a flared
shade stood 0.155 mm below its lowest section on a line, and `stands_on` read
0 mm². A print bed needs the flat ring, which is a cut: a slab off each end,
`shade.thicken(t).cut(slab.at(0, 0, rim - 10), ...)`.

### `clearance` is a word a lamp script reaches for

It is the fastener table's function, so `const clearance = wall + 0.8` does
not parse. Every export is a reserved word; name a local for what it is,
`closest`. Since 2026-09-18 the refusal names the word — the engine's own
said `Cannot declare a const variable twice`, or under QuickJS `invalid
redefinition of parameter name`, and a coin-holder session read that as a
mystery — proved by compiling the script without each name
(`__parcadShadowedBuiltins` in `dsl.ts`, docs/DSL_GAPS.md §7).

### `offset_surface` lies

It returns valid-looking wrong answers rather than failing:

- On a union it silently **drops bodies** — a 44×24×34 part came back 20×20×34.
  `clean()` does not help.
- `offset_surface(+3)` can return an **inside-out** solid; the next offset then
  runs backwards (74×49×32 instead of 66×41×24).
- On **any body with a fillet on it** it returns that inside-out solid every
  time: right size, right shape, every face pointing in, and a later cut or
  union reads it as all of space minus the part — `box(50,30,20).edges("|Z")
  .fillet(5).offset(1)` cut *nothing* out of a block until this was caught.
  The bounding box cannot see it. `Shape::signed_volume` (`BRepGProp`) can:
  it comes back negative, and `facing_outward` in `backend.rs` finds and
  turns it (docs/VALIDITY_CHECKS.md); `ShapeFix` does not. Feed the builder the
  solid, not the compound a treatment wraps it in — `single_solid()` first —
  or the offset of a compound is a compound and the reorientation passes it
  through untouched.

This is why every offset carries a bounding-box post-condition *and* a sign
check. Don't remove either.

### A filleted box is a `Compound`, not a `Solid`

`BRepFilletAPI_MakeFillet` returns a compound holding one solid. A subsequent
boolean against it **succeeds and produces nothing** — the error surfaces much
later as "the result has no faces". Unwrap with `Shape::single_solid()`.

### `Rotate` and `Scale` don't commute with translation accumulation

`backend.rs` peels translations off children to accumulate offsets. Rotation and
scale must therefore build their child **at the origin** and translate
afterwards.

### The reported tolerance is not the one you asked for

`Request::deflection` is **advisory**. The vendored bindings hard-code 0.01 mm in
`Mesher::new`. `Success::deflection_mm` reports what the mesher *actually* used —
report that, never the request. (We once claimed 0.050 mm quality while meshing
at 0.010.)

### A blended union of face-touching solids is refused — it used to kill the kernel

Two solids that meet *exactly* on a plane — a hub standing on a flange face, a
gusset landing on a plate — cannot be blended at any radius: the seam has no
corner for a fillet to roll along, and OCCT raises instead of building. That
raise used to escape the bridge and take the worker with it (`SIGABRT`, blamed on
the radius); the fillet boundary now catches it, and the refusal names the
tangency, the overlap workaround, and — probed, not guessed — that no smaller
radius built either. Overlap the solids: bury the hub a few millimetres into the
plate, or blend two solids and union the third on unblended.
`examples/flange.js` and `examples/motor-mount.js` carry the workaround with a
comment; docs/DSL_GAPS.md §4 has the full table of shapes that trigger it.
`eval/cases/refuse-tangent-blend-union.json` pins the refusal, keyed on the
sentence our tangency detection writes, so it also goes red if an OCCT upgrade
rewords the raise that detection reads.

### A boss that pokes out of the far face gives the blend a second seam

A boss placed on the plate's *underside* plane rather than its top passes through
and stands out of the far face by exactly the depth it was meant to be buried:
the union then has two seams, the intended one on top and one round a stub
underneath, and a fillet cannot reach further along the stub than the stub goes.
That is the real ceiling behind a table this entry once carried — 1.88 mm on a
2 mm stub, 3.88 mm on 4 mm — and it reads convincingly as "the blend must be
smaller than the overlap". It is not: with the boss ending *inside* the plate
there is one seam and the radius does not depend on the depth at all, a Ø12 cone
buried 0.5, 1 or 2 mm into a 6 mm plate taking a 4 mm blend every time.

The tell was the bounding box: the part measured 45.08 mm tall and its lowest z
was −5.5, and nobody read the low end because the height matched a plausible sum
— read both ends. And a refusal that names a radius on a seam you did not intend
is telling you about the seam, not the radius. `examples/plate-stand.js` carried
the stubs for one commit; its pegs are now on the top face and buried 3 mm.

### A blend that ends on a face it is tangent to — fixed, and worth knowing anyway

The sibling of the case above, and the origin of a vendored kernel patch. A boss
standing on a plate exactly as wide as itself puts the blend's end against a face
it touches without crossing; stock OpenCASCADE returns `IsDone() == true` and a
solid that will not close. Found rebuilding a Fusion 360 part
(`reference/retainer.js`, gitignored with the export it was measured against):
**22 open edges**, and it looked right in the viewport, exported and measured.

**The trigger is tangency, not the curved seam and not the run-off.** Sweeping
the clearance between a boss and the plate's side walls, on a 20 mm version of
the same shape — every row measured, `blend: 2` throughout, current kernel:

| clearance | result |
|---|---|
| 0 (tangent) | valid, watertight — **fixed** by `vendor/occt-sys/patches/0001-tangent-pinch-corner.patch` |
| 1e-9, 1e-7 | valid, watertight — same fix (the union still builds the tangency vertex here) |
| 3.8e-5 .. 1e-4 | valid, watertight — always was |
| 3e-4 .. 2e-3 | refused, naming a measured fallback (0.25 mm builds at 3e-4, 1.82 mm at 2e-3); a caught SIGABRT before the boundary learned to catch |
| 3e-3 .. 1.999 | valid, watertight — the fillet runs 1.5 mm off the plate and is trimmed |
| 2.0 (= the radius) | invalid, refused by the validity gate (unchanged) |
| 2.001 and up | valid, watertight |

The fillet's width at angle θ is bounded by the plate's side plane, at radial
distance `R/|cos θ|` from the boss axis; at the tangency that equals `R` exactly,
so the strip pinches to zero width. It still has a valid topology — the toroidal
face trimmed by the wall plane, ending at an ordinary vertex where the trim curve
meets the inner contact circle at a finite angle. OCCT's own walking already
computed those points, then its generic corner code threw them away and filled
the corner with a GeomPlate patch; the patch replaces that corner treatment for
this configuration and nothing else. On the 20 mm shape the blend now adds
47.50 mm³ of fillet against the 5.17 the broken cap added, inside an exact
20 × 40 × 30 box, and the full retainer measures 143829.66 mm³ against the
reference B-rep's 143825.6.

**Open edges alone are not a success criterion.** At radius >= 3 this operation
once destroyed 23% of the retainer's volume *while reducing* the open-edge count.
Check volume too — that is why `check_blend` pairs containment with
`BRepCheck_Analyzer`, and why the worker refuses any welded mesh with an open
edge as a backstop that does not depend on knowing the cause. The face-touching
family above is a different defect, still refused rather than fixed
(docs/DSL_GAPS.md §4). `eval/cases/tangent-blend.json` and
`eval/cases/tangent-blend-retainer.json` hold the fix down;
`eval/cases/blend-runs-off-the-edge.json` remains the control proving overrun was
never the problem.

### `ThruSections` quietly untwisted a loft — `CheckCompatibility` re-origins wires

A ruled loft between a square and the same square listed a quarter turn on should
twist 90° over its length — that pairing is the wall definition. With
`CheckCompatibility(true)` (which the vendored wrapper used to set, following
upstream), the builder re-origins the section wires to *minimise* twist first,
and the same two sections came back as a straight prism: right height, right
sections, 3000 mm³ instead of 2000, no error anywhere. Found recreating
UnTriangle v3, whose whole geometry is that twist.

`Solid::loft_sections` now passes `CheckCompatibility(false)` — the authored
vertex order *is* the pairing — and `validate_loft` requires every section to
carry the same point count, because with the compatibility pass off OCCT no
longer invents a correspondence for mismatched wires.
`eval/cases/twisted-loft.json` holds the volume against the closed form (⅔·a²·L:
the twisted bar is two thirds of its prism), the tripwire for this pass ever
being turned back on.

### `adjacentTo: { faceNormal }` also matches a hole's own wall

A rim edge borders two faces: the flat face it sits in, and the cylindrical wall
of the hole. The wall answers to axis-aligned normals, so both end rims of a bore
drilled along X match `adjacentTo: { faceNormal: "+z" }`. On a part with
cross-drillings — `examples/manifold-block.js` — the "opens onto the top face"
query silently picked up two extra rims. Use `at: { z: "max" }` there; the
face-normal form is fine when every hole is drilled along one axis.

### A name on a face ends at `clean()`, and a moved copy is not the face it copies

Two ways face provenance died before it worked, both measured on the V holder:

- **The boolean's history stops at the boolean.** `unified()` runs
  `ShapeUpgrade_UnifySameDomain` afterwards, and the coplanar faces it merges —
  a cup's side face with the fan's — are new faces the boolean never saw. The
  cup's outer corner had no tagged face and `{ on: "cup" }` missed it. The fix
  is `Shape::into_unified`, which keeps the unify pass's own
  `BRepTools_History`, and `unified_tracked` in `backend.rs`, which follows
  every name through it.
- **`Modified(S)` answers only for the sub-shape it was given.** Carrying a
  tracked face through a rotation by transforming it separately produces an
  exactly placed *copy*; the next boolean reports it neither modified nor
  deleted, the copy survives with its old boundary, and after the union it
  matches nothing. `through_transform` therefore trades each moved copy for
  the transformed shape's own face or edge with the same geometry before any
  later operation asks. The mirrored cup is what found this: the unmirrored
  one kept its name and the mirrored one lost it at the same union.

### `generatedBy` names the cut, not the tool

Lineage records the boolean node's tag, so `blank.cut(grooves, shaftBore, grub)`
gives all three tools one name and `generatedBy: "bore"` — the tool's own tag —
matches nothing at all. `timing-pulley.js` asked for `{ generatedBy: "machined",
curve: "circle", role: "hole", at: { z: "min" } }` and passed `expect({ count: 1
})` only because the twenty groove rims beside the bore rim were split into open
arcs by coplanar face splits, and `role: "hole"` wants a closed circle. The
backend now merges those faces (`unified`, in `backend.rs`), the arcs close, and
the same query matches 21 — the assertion had been resting on how fragmented the
topology happened to be. Cut in one tagged step per feature, as `knurled-knob.js`
does, and the name means something.

That merge moves counts elsewhere too, always downward: the D-bore lead-in in
`knurled-knob.js` used to select five arc fragments and now selects the two
curves they always described. A count over fragments is a count over how the
kernel happened to split a face — assert over features.

### A cutter coplanar with the face it cuts loses the rim

`countersink()` first built its cone with the wide end exactly on the top face —
where a countersink geometrically ends. OCCT merged the cone's flat top into that
face, and the rim stopped being an edge the cut had generated: the selector
matched one edge instead of five, and no dimension looked wrong. The helper now
builds the cone 0.5 mm taller and wider along its own taper, so the section at
the face is still the called-out head diameter. Same rule as every cutter in
`examples/`: run past the material.

### A cut needs overlength at both ends, and the entry end is the silent one

The rule is usually stated for where a cutter *exits*: run past the material,
because a tool ending exactly on the face it leaves through makes a
zero-thickness sliver. Nothing stated it for where a cutter *enters*, because
until `display-bezel.js` every seeded part cut through something. An external
session recessing glass panels into a model car found the other half: its
cutters' outer faces were meant to lie on the body surface, and it shipped a
**0.004 mm** feather edge that `measure_wall_thickness` found afterwards and
nothing caught when it was made.

**Exact coincidence is not the trap — it is the safe case.** A 60 × 40 × 20
plate, a 30 × 20 pocket 4 mm deep, and the cutter's outer face put at four
distances from the front face it enters:

| the cutter's outer face | volume | faces | mesh | thinnest wall |
|---|---|---|---|---|
| exactly on the face | 45600.00 mm³ | 11 | 28 tri, watertight | 10.000 mm |
| 3 mm proud | 45600.00 mm³ | 11 | 28 tri, watertight | 10.000 mm |
| 0.001 mm short | 45600.60 mm³ | 12 | 24 tri, watertight | 0.001 mm |
| 0.004 mm short | 45602.40 mm³ | 12 | 24 tri, watertight | 0.004 mm |
| 0.1 mm short | 45660.00 mm³ | 12 | 24 tri, watertight | 0.100 mm |

The bottom three rows are not a pocket with a thin lid over it; **they are not a
pocket at all.** The part is a solid block with a sealed void inside it, the
front face is unbroken, and the extra volume is the lid. Nothing that reads like
a failure moves: same bounding box, same watertight mesh, fewer triangles than
the correct part. What moves is the volume, up by the lid, and the face count —
also up, because a void adds surfaces rather than removing them.

**The microns come from arithmetic, not from typing them**, so the trap needs a
face that is not axis-aligned. Take a block whose front face rises 20 mm over
85 mm and a 4 mm panel recessed into it. Write the gradient the way a sketch
reads it — `0.235`, where the exact value is `20 / 85 = 0.23529…` — and the
recess's outer edge runs *inside* the face by `(65 − x) · 0.000294 · cos 13.24°`:
0.0029 mm at one end of the cut and 0.019 mm at the other. OCCT builds it,
reports watertight and 11 faces, and removes 4519.86 mm³ against the
parallelogram's exact 4519.87 — the cutter's own volume and not a micron more, so
the membrane is intact and nothing in the reply mentions it.

**The measurement that finds it used to miss it.** `measure_wall_thickness`
fired from sampled surface points, so it reported a membrane only when a sample
landed on one: on the sloped case above, when it sampled seven rendered views at
96 px it reported a minimum of 10.82 mm and *nothing* under a 1 mm threshold,
while 256 px reported 0.0104 mm — the formula's value there to six places. A
membrane is a wall between two faces that share no edge, and those are now
searched for on the exact surfaces whatever the sample count, and read where
they are thinnest; a sliver whose faces meet is a feather, found from the edge
itself and reported at 0. `eval/cases/pierced-membrane.json` holds a 0.004 mm
lid at 200 samples. docs/PERCEPTION.md §5, "What is certain", has what still
rests on the samples.

So the rule has two ends: **a cutter crosses every face it meets** — past the
material where it exits, proud of the material where it enters. `holeFor` and
`countersink` already do it at 0.5 mm; `examples/display-bezel.js` does it on a
recess, at the entry of its seat and at both ends of its aperture.

### The cut that seals a void is refused; coincidence itself is not, and the table above is why

The kernel refuses one shape of this defect, and it is worth being exact about
which. It does **not** refuse on coincidence, or on any measured clearance. It
counts closed shells across a subtract: a cut that *adds* an internal void has
broken through no face and removed nothing reachable, so the result is a solid
with a cavity sealed inside it — watertight, plausible in every render,
unmanufacturable. There is no threshold in that rule, which is what makes it
safe; row 1 of the table stays at zero voids and goes on building.
`crates/parcad-occt/src/backend.rs` carries it;
`eval/cases/refuse-sealed-void.json` and `coincident-cutter-entry.json` pin the
accident and the safe row against each other.

One limit, real. It catches a cut that closes behind itself, not a cut that
lands thin: a blind hole one micron shy of breaking through the *far* face is
an ordinary blind hole by every topological measure, and stays silent.

Nothing broader fires, and both reasons still hold. Row 1 of the table is correct
geometry — what a boss trimmed back to a face or a slot cut flush with an
underside produces, and `examples/v-block.js` ships one — so a refusal on
coincidence would refuse a legitimate part; and what is actually wrong is
*near*-coincidence, a continuum where 0.004 mm is an accident, 0.4 mm is a
design, and any threshold between them is a number some part reaches
legitimately.

Measuring the outcome instead of the cause fails on trust, not cost: the wall
sweep runs in 10–40 ms at 96 px, but swept over `examples/` it returns
**0.0055 mm** for `hydraulic-line.js` and **0.0069 mm** for `timing-pulley.js`,
both correct parts. Two false alarms, two causes, one fixable. The hydraulic-line
sample sits at z = 20.000, where the bend's arc is trimmed by its own end plane:
both fields are zero, `max` ties, and the gradient comes back as the *plane's*
normal while the tube surface there is vertical, so the ray runs along the face
and the crossing is f32 noise — 0.0055 mm at 96 px, 0.0096 mm at 256 px. (A real
feature reports the same number twice; `thickness.rs` drops *collapsed* gradients
via `GRADIENT_TOLERANCE`, but the wrong branch of a `max` has a perfectly good
unit one.) The other cause cannot be fixed: a tangential feature genuinely has no
minimum wall. The ring between that boss and its O-ring groove is
`0.5 − √(1 − (x − 4)²)` mm thick — x along the boss axis, the torus centred at
x = 4, 1 mm minor radius — so it tapers to zero at the rim, and sampling nearer
returns a smaller number without limit. Every groove, fillet and blend that runs
off an edge does it.

Two false alarms in twenty-one shipped parts teaches an agent to ignore the line,
which is why there is no thin-wall number in every reply: "the thinnest material
anywhere" is not the question "is there a wall here too thin to make". The shell
count escapes that by having a discrete answer where a measurement would need a
threshold nobody can defend. Where a defect can be restated that way it can be
refused; where it cannot, the entry-side rule stays in the examples, in this
file, and in `display-bezel.js`'s comments.

### A multi-tool cut is one cut

`outside.cut(cavity, bore)` — a cavity sealed 3 mm inside every face, and a bore
through the top that opens it — was **refused** as a sealed void, while
`outside.cut(bore, cavity)` built the identical part. The cut ran one boolean
per tool and judged each intermediate, so the cavity was condemned before the
bore reached it. The same fold refused `cut(box(4, 4, 4), box(10, 10, 20))` as a
sealed void and the reverse order as a tool that "meets nowhere", and a blended
`union(left, right, plate)` failed because the two bosses fused first share no
seam to blend.

Now a union or a cut applies all its tools before anything is judged
(`boolean_in_layers` in `backend.rs`): the void count is taken on the result,
a tool is a miss only when it left no face *and* is clear of the original
material (a tool inside another tool's volume is not a miss), and a blend
rounds the whole seam once. `eval/cases/cut-opens-its-own-cavity.json`,
`cut-with-a-redundant-tool.json`, `blended-union-any-order.json` and the two
`refuse-multi-tool-*` cases pin it.

Tools whose boxes are clear of each other go into **one** OCCT boolean: 144
holes in a plate built in 70–100 ms against 1.3–4 s one at a time, 64 bosses in
25–45 ms against 0.3–0.9 s. Tools that overlap go into successive booleans,
because OCCT also intersects the tools of one boolean with each other: forty
slots crossing at a centre took **70–140 s** as one boolean and 1 s in turn.
Intersection stays a fold: `BRepAlgoAPI_Common` with several tools intersects
with their *union*, and an empty common at any step is empty at the end
whatever the order, so its refusal was never order-dependent.

Two things are still order-dependent, deliberately. A chain is several nodes:
`outside.cut(cavity).cut(bore)` still refuses, because `outside.cut(cavity)` is a
shape the script made and could return — list both tools in one `cut`. And the
fold's old hazard, "A union of pieces that do not touch each other kills the
fuse" below, did not reproduce on OCCT 8 in either form (three quarter-torus
arcs listed before four runs build in 27–29 ms either way), but a layer can
still hold disjoint pieces.

### A union that drops solids

Two domes 9 mm across (a revolved smootherstep, flat where it meets the floor)
unioned onto a plate 8.64 mm apart, so their feet overlap by 0.36 mm: the fuse
returned a watertight, valid plate with **one** dome, 1643.64 mm³ and 7 faces,
and raised no alert. 8.4 mm apart it builds both (1687.19 mm³, 8 faces), and
so does 8.0, 7 and 6; spheres at the same places always build. What matters is
two surfaces meeting at a grazing angle over a sliver of overlap, not the count:
a planter saucer with 48 such bumps, in rings whose neighbours sat 8.64 apart,
lost 19 of them, and a hex grid 8 apart lost the dish and came back as 15
separate bumps.

Nothing in the kernel's own reporting is enough to catch it. On the saucer the
boolean's history said no bump was deleted and `DumpWarnings` was empty; on the
hex grid one layer reported `BOPAlgo_AlertFaceBuilderUnusedEdges` and the rest
of the loss was silent. Unioning the bumps together first, one at a time, or
with a 0.3 mm vertical step at the foot lost them too.

So a union is checked on its result (`require_inputs_kept` in `backend.rs`):
every point of an input's faces is inside or on the union of it with
anything, and a 3 × 3 grid per input face is classified against the result at
1e-3 mm. A point outside refuses the union, naming the inputs lost and any
alert the kernel raised. With a blend the check reads the boolean before the
blend, which may take material off a convex seam. It costs 150 ms of 2.1 s on
the twisted planter's 38-input saucer and 20 ms on 48 spheres, paid once per
build. `eval/cases/refuse-union-grazing-domes.json` holds it. Which OCCT step
loses the input is not found yet.

### `role: "hole"` does not match a conical opening

A countersink rim is an inner boundary of the top face by any reading, and
`role: "hole"` drops it — `cover-plate.js` selects on `curve` and position
instead. Related to the D-bore case in docs/DSL_GAPS.md §6: the term matches a
narrower thing than its name suggests.

### A clamped primitive has no gradient inside itself

`cuboid` is the standard exact form: `length3` of the three clamped-positive
terms, plus the negative interior term. Inside the solid all three clamp to zero,
so the exterior half is `sqrt(0)` — and the derivative of `sqrt` at zero is a
division by zero. The gradient is **NaN throughout the interior**, not merely
undefined on the surface. `cylinder` is built the same way and has it too.

Pinned rather than fixed because it is measured not to reach output. Both
consumers of the gradient normalise behind `len > 1e-9`, which NaN fails, so a
NaN becomes the `[0, 0, 1]` fallback instead of a NaN normal; and `faceted`
samples off the crease toward each triangle's own centroid, which lands outside
the solid. `a_box_is_shaded_by_its_own_faces` measures the cost on a real mesh —
0 of 1152 shading normals wrong, checking the 576+ that sit unambiguously
mid-wall. The weak version of that test passes while proving nothing:
`[0, 0, 1]` *is* a legitimate top-face normal, so counting axis-aligned normals
finds no fault however broken the shading is; it has to be checked against the
wall each vertex is actually on. Fixing the NaN means an epsilon under the
`sqrt`, which moves the zero level set everywhere — a deliberate change to make
against `eval/cases/`, not a drive-by.

### `.at(x, y, h)` vs `.at(x, y, h - wall)`

A latent bug in an example: placing a lid at `h` instead of `h - wall` sealed the
box in *both* backends, so nothing looked wrong. Geometry that is subtly wrong in
the same way everywhere is the hardest kind to see.

## Rust/wrapper

- `offset_surface` **consumes `self`**. The fork adds `impl Clone for Shape`
  (cheap — `TopoDS_Shape` is a handle onto a refcounted `TShape`).
- `Shape.inner` is `pub(crate)` upstream, which is the *only* reason we forked.
  `opencascade-sys` already binds every OCCT call we needed.
- TypeScript: `isLineSegments` doesn't exist on the intersected three.js type.
  Use `isLine` — `LineSegments extends Line`.
- `Shape::write_stl` **re-meshed at 0.001 mm** — ten times finer than the
  0.01 mm the worker had just meshed at, so every face was triangulated twice
  and the second pass cost 1.1 s per filleted boss. Eighteen bosses spent the
  kernel's whole 20 s budget writing a file for a slicer, and the timeout blamed
  "an unknown operation". The writer now takes the tolerance; the worker passes
  the mesher's (`vendor/opencascade/PARCAD-CHANGES.md` has it). The CLI and the
  app no longer ask for that file at all: both write the welded viewport mesh
  themselves, binary, a sixth of the bytes and the same triangles on screen and
  in the slicer.

## One malformed tool schema hides the entire MCP surface

`probe_step_export` returned `Json<serde_json::Value>`. A `Value` has no schema,
so schemars emitted an output schema with no `"type"`, and a client that
validates `tools/list` rejects **the whole array** over one bad entry. Every
model saw no parcad tools at all, for a day, while the server went on answering
`tools/list` correctly to anything that asked it directly. Fixed in `c426c465`;
`every_advertised_schema_is_an_object` in `mcp.rs` is the regression, and it
covers input schemas too because a client validates the whole descriptor.

**`--mcp-config` cannot report it** — it says `connected` and swallows the parse
error. `claude mcp list` prints it, and `CLAUDE_CONFIG_DIR` keeps the probe out
of the real config:

```bash
CLAUDE_CONFIG_DIR=/tmp/probe-cfg claude mcp add --transport http parcad http://127.0.0.1:4344/mcp
CLAUDE_CONFIG_DIR=/tmp/probe-cfg claude mcp list
```

**From the field suite it reads as "no model can use these tools."** The tell is
uniformity: every trial failing the same way in both models and both arms, which
a real distribution does not do.

## A tools/list reply without `ttlMs` and `cacheScope` hides the surface too

Same symptom as the malformed schema above, different cause, found the same
way. Claude Code 2.1.268 negotiates MCP protocol 2026-07-28 and validates
every `tools/list` reply against a schema in which `ttlMs` (a number) and
`cacheScope` (`public` or `private`) are required; rmcp 3.1 leaves both off
the wire when unset. `--mcp-config` said `connected`, the model saw no
parcad tools, every field trial failed the same way, and curl saw fifteen
tools. `claude mcp list` did not show it either this time — only
`claude -p --debug`, in `~/.claude/debug/`, with `tools/list failed (Invalid
result …)`. `mcp.rs` now writes its own `list_tools` with both fields;
`the_tool_list_carries_a_ttl_on_the_wire` pins them.

Found beside it: the client truncates server `instructions` at 2048
characters and says so only in that debug log. Ours were 3511, and the
paragraph that fell off was the one about selectors.
`the_instructions_fit_the_client_window` holds the length.

## Protocol 2026-07-28 has no session, and refuses a request that acts as if it did

The other side of the same version, met writing `parcad call`. rmcp 3.1
serves 2026-07-28 statelessly (SEP-2567): there is no `Mcp-Session-Id`, and
every request after `initialize` must carry, in `params._meta`,
`io.modelcontextprotocol/protocolVersion` and
`io.modelcontextprotocol/clientCapabilities`, or it is refused with
"request _meta is missing". Every POST must also repeat its method in an
`Mcp-Method` header and, for `tools/call`, the tool in `Mcp-Name`
(SEP-2243) — the body alone gets "missing required Mcp-Method header". Both
rules are gated on the `MCP-Protocol-Version` header the client sends; the
older versions keep their session and want neither. `call.rs` in the CLI
speaks the new one only, and says so.

## A tool reply over 50,000 characters reaches the model as a 2 KB preview

Claude Code 2.1.273 counts a tool result's text in **characters, not bytes**,
and at **50,000** it stops handing the text to the model: longer than that,
the result is written to a file under
`~/.claude/projects/…/tool-results/` and the model gets a
`<persisted-output>` note saying `Output too large (NN.NKB)` with the first
2 KB. The KB there is characters ÷ 1024. A model with no Read tool — every
field trial, and any client that denies it — cannot open that file, so the
rest of the reply does not exist for it. Nothing fails: the call succeeds,
and the model works from whatever else it has.

Measured on 2026-09-16 with a one-tool stdio server returning `n` copies of
one character, asked once per `n` by haiku:

| text returned | reached the model |
|---|---|
| 49,000 × `x` | whole |
| 50,000 × `x` | whole |
| 50,001 × `x` | persisted, 2,368-character preview |
| 51,000 × `x` | persisted |
| 50,000 × `—` (150,000 bytes) | whole |
| 30,000 × `é` (60,000 bytes) | whole |

A second form hides a reply completely: `Error: result (61,261 characters)
exceeds maximum allowed tokens. Output has been saved to …`, with no preview.
Sonnet got it for the gears branch's 61,261-character `dsl` topic, where haiku
got the preview form, and sonnet got the preview form for a 53,601-character
reply. The client's token cap, `MAX_MCP_OUTPUT_TOKENS` (25,000 by default), is
the likely trigger; that is not measured. Handed the error, sonnet went looking
for PowerShell, Grep or Read in 10 of 12 trials.

For a tool that returns a struct through rmcp's `Json`, the counted text is
the compact JSON (`Value::to_string()`, non-ASCII unescaped), so a newline in
a document costs two characters and a quote costs two. `read_docs` was the
tool it hid: the `dsl` topic ran 58–62 KB on the gears branch, and in
`eval/field/how-close-to-the-involute.md` every trial got the preview, read a
seeded part's source instead, and graded LUCKY. `gotchas` was 53,601 on main.
`docs.rs` now serves any topic longer than `SECTION_BUDGET_CHARS` (12,000
escaped — the client limit is a ceiling, not a target) as its contents and then one `section` per call;
`every_reply_fits_in_what_a_client_shows` holds every reply under
`CLIENT_RESULT_LIMIT_CHARS`. Any other tool that can grow — a long
`list_entities`, a large probe — has the same ceiling. The field grader no longer counts such a call as
reaching its tool, and shows how many replies were hidden: field/README.md,
"A call is not a read".

## An MCP argument the host did not read was dropped, not refused *(fixed 2026-09-18)*

No request struct in `mcp.rs` carried `deny_unknown_fields`, so serde dropped
a field it had no slot for and the call went ahead without it. The coin-holder
session (docs/COIN_HOLDER_REVIEW.md, L4) asked `evaluate_part` for
`section: { axis: "x", offset: -38 }`: `offset` is not a field — `at_mm` is —
so the cut went through the middle of the part, and the model reasoned about
coin fit from that picture. The reply's resolved `section` said where the cut
really was, and nothing read it. A wrong measurement, not a retry, which is
the class of failure this file exists for.

Every request struct now refuses an unknown field, and `arguments::Args`
names the field that was meant (`offset` → `at_mm`) and the shape of the whole
argument, from the schema `tools/list` publishes, so the two cannot drift. The
same rule one layer down is `envelope::check_node`, which refuses a graph
field the host would silently drop. `eval/field/where-was-it-cut.md` measures
whether a model reads the refusal.

## A union of pieces that do not touch each other kills the fuse that joins them

`pipe()` builds a tube as runs and bend arcs and unions them. Assembled with
every arc first and the straight runs afterwards, OCCT **hung** on a two-bend
route and **segfaulted** on a three-bend one; assembled in path order — run, arc,
run, arc — the same route builds in milliseconds.

The fold is pairwise. Arcs at different corners are disjoint solids, so fusing
them first produces a compound of separate lumps, and the boolean that finally
bridges them has to resolve every contact at once; fusing along the chain means
each step touches what is already there. **The rule:** when unioning a chain of
shapes, union them in the order they touch. Cheap to get right and expensive to
debug, because the failure is a SIGSEGV three operations later.

## A coaxial torus groove in a cylinder segfaulted `UnifySameDomain` *(fixed)*

```js
cylinder(12, 14).cut(torus(11.5, 1))   // SIGSEGV, before OCCT 8.0.1
```

That is an O-ring groove, which is the shape a torus exists for. The **boolean
was fine** — the crash was in `unified()`, the `clean()` / `UnifySameDomain` pass
every result goes through to weld away imprint edges, and it died on the two
coaxial circular seams the cut leaves in the cylinder wall.

What did *not* crash, kept because it is how the next crash of this family gets
narrowed down:

| shape | result |
|---|---|
| `box(30, 30, 14).cut(torus(11.5, 1))` | fine |
| `cylinder(12, 14).cut(torus(20, 4))` — enters from the side | fine |
| `cylinder(12, 14).cut(cylinder(11, 20))` — coaxial, no torus | fine |
| `cylinder(12, 14).cut(torus(11.5, 1))` | **SIGSEGV** |

**Vendoring OCCT 8.0.1 fixed it**, and the `known_defect` marker came off
`eval/cases/torus-gland.json` — that marker working as designed rather than a
case being quietly relaxed. The case still runs, so the fix cannot silently
regress, and `examples/hydraulic-line.js` now carries the groove in a real part.
What that part cannot do is a *catalogue* gland: a torus cut is a circle in
section, and a standard gland is rectangular and wider than the cord. The blocker
moved from the kernel to the section — docs/DSL_GAPS.md, "arcs in a section".

## A release build of the app compiled the host three times

Measured on 2026-09-14, from an empty `target/`, running release.yml's cargo
steps in order on a 14-core M4 Pro (two runs each):

| | `tauri build` step | whole sequence |
|---|---|---|
| as it was | 213 s, 193 s | 326 s, 294 s |
| `crate-type = ["rlib"]` only | 143 s, 145 s | 225 s, 229 s |
| and the CLI in tauri's own cargo invocation | 81 s, 80 s | 191 s, 185 s |

Two independent causes. `parcad-app` declared `["staticlib", "cdylib", "rlib"]`,
Tauri's template default: the first two exist for iOS and Android, and on a
desktop build each is a full thin-LTO link of the whole app. And
`beforeBuildCommand` built `parcad-cli` in its own `cargo build` before tauri
ran another for `parcad-app`. Features unify per invocation, so the second saw
different features on `serde`, `libc`, `syn`, `tokio` and 94 more crates and
rebuilt all of them — and a later `cargo build -p parcad-cli` step rebuilt 93
again for the same reason.

CI now overrides `beforeBuildCommand` to the frontend alone and hands tauri
`-- -p parcad-app -p parcad-cli`, so one invocation builds both. The override is
CI's, not tauri.conf.json's: a plain `bun run tauri build` still has to produce
the `parcad` the macOS bundle carries, and `tauri dev` reads the same runner
configuration. Do not add a `cargo build -p parcad-cli` after it; assert the
binary exists instead.

## A fit's reference was built from the part's cache

Found on 2026-09-17, and there since the warm worker (`fdd010e7`,
2026-09-14). `check_fit` builds the part and then the reference inside one
build-cache scope, and the memo of subtree keys that scope keeps is indexed by
node id and was filled by the part: the reference's node `i` was looked up
under the part's node `i`.

| `box(40,40,10).cut(box(20,20,20))` against | answered | right |
|---|---|---|
| `box(19.5,19.5,30)` | interfering by 12000 mm³: the plate, the part's node 0 | clear by 0.25 mm |
| the same `.at(0,0,0.001)` | touching: the 20 mm cutter, the part's node 1 | clear by 0.25 mm |

A cold worker answered the same: the part fills the cache in the same request.
It is a hit only where the part built that node at the same offset, so a
reference placed where the part built nothing came back right — the laptop in
`does-the-laptop-fit` read 0.5 mm on the old worker — and was then kept under
the part's keys. After fitting `box(40,40,10).cut(box(20,20,20).at(0,0,3))`
against `sphere(4).at(0,0,3)`, which answered correctly, the next
`evaluate_part` of `box(40,40,10).cut(box(20,20,20))` on that worker cut the
ball: 15774.534 mm³ and 7 faces for 12000 and 10.

A node id means something only in its own document. The scope now records the
document its memo belongs to, a node of any other document is built without the
cache, and `check_fit` builds its reference under a memo of its own
(`in_document` in `backend.rs`), so both are reused. Nothing had caught it
because nothing asked: no corpus case called `check_fit`, `between_bodies`
measures the bodies of one document, and the unit test called
`backend::check_fit` with no cache installed. `eval/cases/fit-in-the-holes.json`
now asks three references twice each on the worker that has just evaluated
the part.

## Parts are not in Documents

Found 2026-09-18: the pencil handed `~/Documents/parcad/bracket.parcad/part.js`
to Cursor, and Cursor answered `NoPermissions (FileSystemError)`. macOS guards
Documents, Desktop, Downloads and iCloud Drive per app (TCC). ParCAD having the
grant says nothing about the editor it hands a file to. That editor was
already running, launched from the Dock, so the read was its own. The same
wall stands in front of every agent, shell and launchd service that reads the
folder. So the folder is `dirs::data_dir()/parcad` — `~/Library/Application
Support/parcad` on macOS — which no app needs a grant for. The files are the
same plain files. Nothing moved an older folder over; one user, done by hand.

## ParCAD web: the host in a tab

Found building `crates/parcad-wasm-host` on 2026-09-17; docs/ARCHITECTURE.md,
"A third host: ParCAD web", is the design these constrain.

### QuickJS's stack check misfires under Emscripten, and then trips too late

With Emscripten's default 64 KB stack, every script failed with "Maximum call
stack size exceeded": QuickJS measures its depth against the stack it starts on,
and a stack that small puts the limit below address zero. The host links a
16 MB stack. Then the opposite failure: QuickJS's recursion nests several wasm
frames per JavaScript call on the *engine's* native stack, which runs out long
before the 16 MB shadow stack does, as a `RangeError` trap that leaves the
module's state unusable. Measured under Node 22, plain self-recursion:

| `set_max_stack_size` | what a runaway recursion gets |
|---|---|
| 64 KB | a clean exception at depth 162 |
| 256 KB | a clean exception at depth 654 |
| 512 KB | the engine's trap |

`script.rs` sets 256 KB on this target. In Chrome's worker the same runaway
recursion came back as QuickJS's clean refusal, not a trap; Firefox and Safari
are not measured. A script that recurses 650 deep builds on the desktop and not
in a tab, and says so in words.

### A tab's call runs twice when it needs the kernel, so it must ask before it writes

`page.rs` pauses a call at its kernel request by unwinding, and runs it again
once the page has the answer. Anything the call wrote before asking is written
again on the second pass. `save_project` used to snapshot and write `part.js`
and then build the thumbnail; its second pass then kept the *new* script as a
snapshot and reported that. It builds the thumbnail first now. A tool that
writes and then builds has the same bug in a tab only.

The second pass also names new scratch files, so an answer is matched to its
request with `step_path` and `stl_path` blanked, and a carried file is written
to the new request's path by what it is (the STEP, the STL), not where it was.

### A call that waits must hand the page its events first

`set_script` with `wait_s` waits for the window to report the new revision —
and the host worker delivered session events only when a call *finished*. So
the page heard of the edit only when some other call ended, which was the agent
chip's status poll four seconds later, and every `set_script` took 3.9 s. The
worker now delivers events before any call waits: 0.14 s, measured through the
relay with the window drawing the part.

### rmcp's HTTP service answers only a loopback `Host`

`StreamableHttpService` checks the `Host` header against an allow-list that
defaults to loopback, and answers anything else with 403 "Host header is not
allowed". The tab has no host name of its own, so `page.rs` builds every request
as `http://localhost/mcp` with `Host: localhost`, whatever the relay received.

### New page script against an old host module reads nonsense

Under `vite --mode web`, rebuilding the host module while a page is open
and letting the page reload can pair the new worker script with the module the
old one fetched. An argument added to an export then lands in the wrong slot:
`host_answer` read a pointer as a length, the reply packet "had 211936156 bytes
past its end", and the call asked the kernel again, forever. A production build
cannot mix them — both files sit under one content hash — and an unreadable
reply now ends the call with "reload the tab", but restart the dev server after
rebuilding either module.

### Two checkouts sharing one `CARGO_TARGET_DIR` overwrite each other

Cargo hashes a path dependency's artifacts by its path *relative to the
workspace root*, so a git worktree building into the main checkout's `target/`
writes the same files. Building the committed code for a benchmark that way
left the main checkout's `parcad-host` compiling against the worktree's
`parcad-occt` ("no `packet` in the root"). `cargo clean -p` the workspace
crates afterwards, or give the worktree its own target directory and reuse only
OpenCASCADE through `PARCAD_OCCT_PREBUILT`.

## The editor does not check the length of an array

`app/src/intellisense/analyzer.ts` drops any diagnostic whose cause bottoms out
in TypeScript's 2620 or 2621 — *"Target requires N element(s) but source may
have fewer"* and its opposite. Both mean the same thing: the compiler agrees the
element types are right and cannot prove how many there are.

A part is JavaScript, so it never can. `hull(points).map(([x, y]) => [x, y * k])`
is a `number[][]`, the DSL wants `[number, number][]`, and there is no
annotation a `.js` file can carry to say otherwise. Before the filter the seed
corpus produced ten of these and no other complaint at all — a checker that is
wrong on every shipped example is worse than no checker, because the first
squiggle a reader dismisses is the last one they read.

What survives is everything that is not about length: a string among the
numbers, a misspelt method, a missing argument, an option key that does not
exist, and a written-out `[0, 0, 0]` where two are wanted, which is code 2618
and still reported. `analyzer.test.ts` runs every part in `examples/` and
requires zero, and runs seven mistakes and requires one each.

## A part in somebody else's editor

`crates/parcad-host/src/editor_types.rs` writes `jsconfig.json` and a `.types/`
folder at the root of the parts directory, so the pencil in the titlebar opens a
file VS Code can hover. Three things about that file were each measured wrong
first:

- **`"include": [".types"]` does not work.** TypeScript will not walk into a
  directory whose name begins with a dot, and says nothing about it — the
  declarations simply resolve to nothing and every DSL name reads as undefined.
  It has to be spelt `".types/**/*"`. The folder stays dotted because
  `projects::walk` skips dotted entries, and a visible one would appear in the
  parts picker as a collection the user did not make.
- **`"moduleDetection": "force"` is load-bearing.** Left as scripts, every
  `part.js` shares one global scope, and the second part to write `const plate`
  redeclares the first. Twenty parts in a folder is twenty files of errors about
  each other.
- **`"checkJs"` is off, and stays off.** A part ends in `return`, which is an
  error outside a function body. The window's own editor compiles the document
  wrapped in one (`intellisense/part-file.ts`); a file on disk cannot be.
  Completion, hover and signature help need no wrapper, and they are the whole
  point of the file.

The declarations are generated from `script::surface()` — the DSL evaluated in
the sandbox — so they are the names `new Function` binds, not the names a parser
found. A `jsconfig.json` without parcad's marker comment belongs to the user and
is never replaced.

## The completion details panel is outside its parent

CodeMirror hangs `.cm-completionInfo` off the completion list as a *child*, and
then positions it entirely beside the list — parent right edge 361, panel left
edge 365, in the measurement that found this. Any `overflow: hidden` on
`.cm-tooltip` therefore does not trim the panel, it deletes it: the layout still
reports a sensible box and sensible text, and nothing is painted.

That is what makes it worth writing down. The panel was invisible for three
commits while a check that read `innerText` off the element reported it working,
because `innerText` cannot see a clip. `document.elementFromPoint` inside the
panel's own box is the test that finds it — it answered `CANVAS`, the viewport
behind.

So the radius is clipped on `.cm-tooltip-hover` and `.cm-completionInfo`, which
need it because a section rule is drawn past their padding, and never on the
list.

## The webview is not the browser you tested in

`requestIdleCallback` does not exist in WKWebView. It is in Safari, it is in
every browser this app is developed against, and it is not in the webview the
desktop app actually ships. Calling it unguarded in `ui/editor.tsx` threw
inside the editor's mount effect, which took the whole Preact tree with it: no
viewport, no parts picker, nothing but a titlebar and a report — because the
report's text comes from a path that had already rendered.

Three things let it through, and all three are worth keeping in mind:

- **Every check was a browser check.** The dev server, the production bundle
  served over HTTP, a second app instance read through Chromium — all of them
  render correctly, because all of them are not WKWebView. The failure needs
  the app's own window.
- **`tools/test-desktop.sh` was not in `tools/check.sh`.** Its own header says
  browser-only tests are "intentionally insufficient for this suite: its
  purpose is to catch failures in Tauri's WebKit host and IPC bridge" — and a
  nine-stage green gate said nothing at all about this.
- **The viewport spec asked what the viewport *knew*, not what it drew.**
  `bracket.e2e.mjs` projects an edge and hovers it, and passes perfectly while
  every mesh is invisible. `draws-the-part.e2e.mjs` renders the scene twice,
  once with the part hidden, and compares the frames; it needs to know nothing
  about the background or the theme, and a scene that draws everything except
  the part cannot satisfy it.

Before reaching for a browser API in the frontend, check it against WKWebView
rather than against caniuse's "Safari" column — the two are not the same
thing. Where one is worth using anyway, guard it and provide the fallback, as
`idle()` in `ui/editor.tsx` does.

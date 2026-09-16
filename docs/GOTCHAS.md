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

### The MCP server only exists while a host is running

It is hosted by the desktop process or by `parcad serve`, never by a binary a
client launches, so a client connects only while one of those is up —
`brew services start parcad` is how it stays up. Point the client at the
streamable-HTTP URL:

```bash
claude mcp add --transport http parcad http://127.0.0.1:4242/mcp
```

A `.mcp.json` in a workspace needs that workspace **explicitly** trusted, and a
workspace under an already-trusted parent inherits trust without the dialog ever
firing — so it can never *become* explicitly trusted, and the server stays
`⏸ Pending approval` forever. Not parcad's bug, and it reads exactly like one;
the fix is `hasTrustDialogAccepted: true` for that path in `~/.claude.json`.

There is no stdio transport on purpose. A stdio server would be a second process
with its own kernel and its own idea of what is on screen, which is the split
`service.rs` exists to prevent.

### `cargo build` needs bun, because the sandbox embeds the DSL

`build.rs` shells out to `bun build` to compile `app/src/dsl.ts` into the script
sandbox. Not a new requirement — the frontend already needed bun — but the
failure now happens during `cargo build` rather than at `bun run tauri dev`. The
panic names the fix.

### `cargo build -p parcad-occt` does not build the worker

Without `--features kernel` you get only the host half and the binary is skipped
entirely. Use `tools/build-worker.sh`.

### A Tauri sidecar is declared, staged and installed under three names

`externalBin` in `tauri.conf.json` names `binaries/parcad-occt-worker`. The file
on disk has to be `binaries/parcad-occt-worker-aarch64-apple-darwin` or the build
fails outright — that part is loud. What is quiet is the third name: the macOS
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

### zsh aborts a command on an unmatched glob

`rm -rf target/release/build/occt-sys-* target/debug/build/occt-sys-*` dies on
the second (already-deleted) glob *before running anything*. Use:

```bash
find target -maxdepth 3 -name "occt-sys-*" -type d -exec rm -rf {} +
```

### The WebAssembly kernel: four ways a build links something else

All met building `playground/build-kernel.sh`; each produced a binary, not an
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
  calls, and wasm has no signals; `playground/occt-emscripten.cmake` undefines it.
- **`--bin` filters every `-p`.** `cargo build -p a --bin x -p b` builds no
  binary of `b`; name each.

## Geometry

### A planar wall meshes two ways

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
or edge count; playground/README.md has the table.

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

### A sideways inset of a leaning wall is thinner than the inset

Stepping a section inward by `t` in its own plane makes a wall `t · cos φ`
thick, square to a surface leaning `φ` from vertical. The fitted lamp shade
built as outer sections minus sections stepped in by 1.6 mm measured 1.211 mm
at its thinnest in `measure_wall_thickness` (kind `wall`, not a rim artifact),
and a horizontal inset of one sloped section measured 1.387 mm. `loft(...,
{ wall })` steps by `t / cos φ` from the built outside, so its `loft_wall_mm`
is the wall square to the surface.

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
(`parcad_core::skin::PeriodicFit`), with no seam at all. A plain `{ fit }`
section in an extrude or a revolve still goes through `Edge::fit`; its loop
check catches a loop, but the seam's speed jump is not reported.

### One large B-spline face meshes far slower than the same surface in bands

The fitted lamp as one smooth outer face and one inner (4 faces) spent 102 s
in the loft's bounding-box tessellation at 0.01 mm; the same two surfaces cut
into a face per stretch between its 15 sections (30 faces) took 18.7 s, and
the mesh after it 1 s. `ThruSections`' single smooth face in the older lamp
took 166 s. Splitting further, in `u` as well, did not help measurably (16 to
19 s under load). So a skinned loft is always banded at its sections, and the
edges between bands are `dihedral: "smooth"`.

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
  it comes back negative, and `Shape::oriented_outward`
  (`BRepLib::OrientClosedSolid`) turns it right side out; `ShapeFix` does
  not. Both are parcad additions to the vendored crates. Feed the builder the
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

**And the measurement that finds it can miss it.** `measure_wall_thickness`
fires from sampled surface points, so it reports a membrane only when a sample
lands on one: on the sloped case above, when it sampled seven rendered views at
96 px it reported a minimum of 10.82 mm and *nothing* under a 1 mm threshold,
while 256 px reported 0.0104 mm — the formula's value there to six places. It
now samples the exact tessellation's nodes and a grid over every triangle, and
the same rule holds: raise `max_samples` before believing a clean answer;
docs/PERCEPTION.md §5 has the other direction it is wrong in.

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

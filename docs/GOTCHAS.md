# Gotchas

Things that cost real time. Each one was silent — everything "worked".

## Build & tooling

### OpenCASCADE compiles at `-O0` unless you stop it

**The single largest performance bug in the project so far.** The `cmake` crate
resolves `CMAKE_BUILD_TYPE` to `Debug` for `occt-sys` *even under
`cargo build --release`*. The Rust half is optimised, the geometry kernel
underneath it is not, and nothing says so. Enclosure build time was **203 ms**;
it should be 43.

Fixed by `cmake/occt-toolchain.cmake`, wired in via `CMAKE_TOOLCHAIN_FILE` in
`.cargo/config.toml`. Note the two traps inside the fix:

- **`CXXFLAGS=-O2` does nothing.** cmake-rs's `skip_arg` strips every `-O`/`/O`/
  `-g` argument from forwarded compiler flags *on purpose*. A toolchain file is
  the one hook it passes through untouched.
- **We force `-O2` inside the Debug configuration rather than switching to
  Release.** OCCT's Release config defines `-DNo_Exception`, which turns
  `Standard_Failure` raises into no-ops — i.e. it converts our catchable
  geometry errors into undefined behaviour. Never enable it.

`tools/build-worker.sh` greps the generated `flags.make` and prints a loud
warning if the optimisation didn't take. Trust that check, not the config file.

Measured (median of 7):

| | `-O0` | `-O2` | `-O3` |
|---|---|---|---|
| enclosure build / mesh | 203 / 22 ms | 43 / 4 ms | 44 / 4 ms |
| bracket build / mesh | 197 / 181 ms | 39 / 28 ms | 39 / 28 ms |
| process floor | 62 ms | 11 ms | 11 ms |
| worker binary | 34.3 MB | 25.5 MB | 26.7 MB |

`-O2` is chosen: identical speed to `-O3`, 1.2 MB smaller. All 11 regression
cases produced byte-identical geometry after the change.

### Run the app from `app/`, not the repo root

`bunx tauri dev` at the repo root fetches `tauri` from npm and dies building
`sharp`/`vips`. And **don't** use `bunx --bun tauri dev` — that fails with
"could not determine executable to run for package tauri".

```bash
cd app && bun install --frozen-lockfile && bun run tauri dev
```

### A second app instance silently has no browser UI

The UI port is held by whichever instance bound it first. The second one prints
why and keeps its desktop window working — but a browser tab is then talking to
the *first* instance's kernel, which is not obviously wrong and is very
confusing. Read the app's stderr; it names the fix:

```bash
PARCAD_HTTP_PORT=4243 cargo run -p parcad-app
```

A browser that loses the host mid-session shows "the parcad desktop process is
not answering" and keeps the last good geometry on screen. That is the app
having exited, not a failed evaluation — `tauri dev` restarting on a rebuild is
the usual cause.

### `cargo build -p parcad-occt` does not build the worker

Without `--features kernel` you get only the host half and the binary is skipped
entirely. Use `tools/build-worker.sh`.

### zsh aborts a command on an unmatched glob

`rm -rf target/release/build/occt-sys-* target/debug/build/occt-sys-*` dies on
the second (already-deleted) glob *before running anything*. Use:

```bash
find target -maxdepth 3 -name "occt-sys-*" -type d -exec rm -rf {} +
```

## Geometry

### `offset_surface` lies

It returns valid-looking wrong answers rather than failing:

- On a union it silently **drops bodies** — a 44×24×34 part came back 20×20×34.
  `clean()` does not help.
- `offset_surface(+3)` can return an **inside-out** solid; the next offset then
  runs backwards (74×49×32 instead of 66×41×24).

This is why every offset carries a bounding-box post-condition. Don't remove it.

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

### A blended union of face-touching solids kills the kernel

Two solids that meet *exactly* on a plane — a hub standing on a flange face, a
gusset landing on a plate — abort OCCT with `SIGABRT` when the union is
blended, at any radius. Overlap them instead: bury the hub a few millimetres
into the plate, or take the blend between two solids and union the third on
unblended. `examples/flange.js` and `examples/motor-mount.js` both carry the
workaround with a comment, and docs/DSL_GAPS.md §4 has the full table of shapes
that trigger it.

The crash is caught and reported with a breadcrumb, so nothing is lost — but
the message blames the radius, and the radius is not the problem.

### `adjacentTo: { faceNormal }` also matches a hole's own wall

A rim edge borders two faces: the flat face it sits in, and the cylindrical
wall of the hole. The wall answers to axis-aligned normals, so both end rims of
a bore drilled along X match `adjacentTo: { faceNormal: "+z" }`. On a part with
cross-drillings — `examples/manifold-block.js` — the "opens onto the top face"
query silently picked up two extra rims. Use `at: { z: "max" }` there; the
face-normal form is fine when every hole is drilled along one axis.

### `.at(x, y, h)` vs `.at(x, y, h - wall)`

A latent bug in `examples.ts`: placing a lid at `h` instead of `h - wall` sealed
the box in *both* backends, so nothing looked wrong. Geometry that is subtly
wrong in the same way everywhere is the hardest kind to see.

## Rust/wrapper

- `offset_surface` **consumes `self`**. The fork adds `impl Clone for Shape`
  (cheap — `TopoDS_Shape` is a handle onto a refcounted `TShape`).
- `Shape.inner` is `pub(crate)` upstream, which is the *only* reason we forked.
  `opencascade-sys` already binds every OCCT call we needed.
- TypeScript: `isLineSegments` doesn't exist on the intersected three.js type.
  Use `isLine` — `LineSegments extends Line`.

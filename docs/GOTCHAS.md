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

### A missing `capabilities/` rebuilt the app on every single build

`cargo build` with nothing changed took **16 s**, every time. Not compilation:
`tauri_build::build()` emits a `rerun-if-changed` for `app/src-tauri/capabilities`,
and cargo treats a `rerun-if-changed` on a *missing* path as permanently stale.
The build script re-ran on every build, which meant the `bun build` of the DSL
bundle re-ran, which invalidated `parcad-app` and everything downstream.

The directory now exists and is deliberately empty of capability files — adding
one would grant permissions the app does not have today. A no-op build is 0.2 s.

Cargo will not tell you this; ask it:

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

The tauri CLI adds `custom-protocol` to the `cargo build` it runs. Nothing else
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
embedded in this binary`, which is the same fact from the other side.

Two dead ends worth not repeating, both eliminated by measurement:

- **Not the `.app` bundle, and not window activation.** Cross the two variables:
  the tauri-built binary in a hand-made minimal `.app` works, and the cargo-built
  binary inside the *real* `ParCAD.app` (re-signed, launched with `open`) fails.
  The binary is the only variable. `NSRunningApplication.activate()` returns
  `true` and changes nothing.
- **Not release-vs-debug.** `tauri build --debug --no-bundle` works because it
  is the *CLI* that builds it, not because of the profile. A plain
  `cargo build` (debug) fails the same way — worse, since dev mode is then the
  profile's doing as well.

The app now says this itself at startup rather than showing a blank window; see
`warn_if_the_window_awaits_a_dev_server` in `lib.rs`. Note what the WebKit log
shows in the failing case, because it misleads: the page load *completes*, then
`view visibility state changed 1 -> 0` and the web process is throttled to
background. That is the consequence of loading a dead URL, not the cause.

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

### The MCP server only exists while the app is running

It is hosted by the desktop process, not by a separate binary, so an MCP client
configured against it connects only when parcad is open. Point a client at the
streamable-HTTP URL:

```bash
claude mcp add --transport http parcad http://127.0.0.1:4242/mcp
```

There is no stdio transport on purpose. A stdio server would be a second process
with its own kernel and its own idea of what is on screen, which is the split
that `service.rs` exists to prevent.

### `cargo build` needs bun, because the sandbox embeds the DSL

`build.rs` shells out to `bun build` to compile `app/src/dsl.ts` into the script
sandbox. The app already needed bun for its frontend, so this is not a new
requirement — but the failure now happens during `cargo build` rather than at
`bun run tauri dev`, which is a surprising place to meet it. The panic names the
fix.

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

### A blend that ends on a face it is tangent to — fixed, and worth knowing anyway

The sibling of the case above, and the origin of a vendored kernel patch. A
boss standing on a plate exactly as wide as itself puts the blend's end against
a face it touches without crossing; stock OpenCASCADE returns `IsDone() ==
true` and a solid that will not close — found while rebuilding a Fusion 360
part (`reference/retainer.js`, gitignored with the export it was measured
against), where the result had **22 open edges**, looked right in the
viewport, exported, measured, and was wrong.

**The trigger is tangency, not the curved seam and not the run-off.** Sweeping
the clearance between a boss and the plate's side walls, on a 20 mm version of
the same shape — every row measured, `blend: 2` throughout, current kernel:

| clearance | result |
|---|---|
| 0 (tangent) | valid, watertight — **fixed** by `vendor/occt-sys/patches/0001-tangent-pinch-corner.patch` |
| 1e-9, 1e-7 | valid, watertight — same fix (the union still builds the tangency vertex here) |
| 3.8e-5 .. 1e-4 | valid, watertight — always was |
| 3e-4 .. 2e-3 | SIGABRT, caught and refused (unchanged, see the face-touching family above) |
| 3e-3 .. 1.999 | valid, watertight — the fillet runs 1.5 mm off the plate and is trimmed |
| 2.0 (= the radius) | invalid, refused by the validity gate (unchanged) |
| 2.001 and up | valid, watertight |

Why tangency was hard: the fillet's width at angle θ is bounded by the plate's
side plane, at radial distance `R/|cos θ|` from the boss axis. At the tangency
that equals `R` exactly, so the strip pinches to zero width. The wrong
conclusion this table originally carried — that the pinch has no valid
topology — did not survive the fix: the correct answer is the toroidal face
trimmed by the wall plane, ending at an ordinary vertex where the trim curve
meets the inner contact circle at a finite angle. OpenCASCADE's own walking
already computed those exact points; its generic corner code then threw them
away and filled the corner with a GeomPlate patch. The patch replaces that
corner treatment for this configuration and nothing else; measured on the
20 mm shape, the blend now adds 47.50 mm³ of fillet against the 5.17 the
broken cap added, inside an exact 20 × 40 × 30 box, and the full retainer
measures 143829.66 mm³ against the reference B-rep's 143825.6.

What stays true and load-bearing:

- **Open edges alone are not a success criterion.** At radius >= 3 this
  operation once destroyed 23% of the retainer's volume *while reducing* the
  open-edge count. Check volume too — that is why `check_blend` pairs
  containment with `BRepCheck_Analyzer`, and why the worker refuses any welded
  mesh with an open edge as a backstop that does not depend on knowing the
  cause.
- **The face-touching SIGABRT family above is a different defect** and is
  still refused, not fixed; docs/DSL_GAPS.md §4 has the table.
- `eval/cases/tangent-blend.json` and `eval/cases/tangent-blend-retainer.json`
  hold the fix down; `eval/cases/blend-runs-off-the-edge.json` remains the
  control proving overrun was never the problem.

### `ThruSections` quietly untwisted a loft — `CheckCompatibility` re-origins wires

A ruled loft between a square and the same square listed a quarter turn on
should twist 90° over its length — that pairing is the wall definition. With
OCCT's `CheckCompatibility(true)` (which the vendored wrapper used to set,
following upstream), the builder re-origins the section wires to *minimise*
twist first, and the same two sections came back as a straight prism: right
height, right sections, 3000 mm³ instead of 2000, and no error anywhere.
Found recreating UnTriangle v3, whose whole geometry is that twist.

`Solid::loft_sections` now passes `CheckCompatibility(false)` — the authored
vertex order *is* the pairing — and `validate_loft` requires every section to
carry the same point count, because with the compatibility pass off, OCCT no
longer invents a correspondence for mismatched wires.
`eval/cases/twisted-loft.json` holds the volume against the closed form
(⅔·a²·L: the twisted bar is two thirds of its prism), which is the tripwire
for this pass ever being turned back on.

### `adjacentTo: { faceNormal }` also matches a hole's own wall

A rim edge borders two faces: the flat face it sits in, and the cylindrical
wall of the hole. The wall answers to axis-aligned normals, so both end rims of
a bore drilled along X match `adjacentTo: { faceNormal: "+z" }`. On a part with
cross-drillings — `examples/manifold-block.js` — the "opens onto the top face"
query silently picked up two extra rims. Use `at: { z: "max" }` there; the
face-normal form is fine when every hole is drilled along one axis.

### `generatedBy` names the cut, not the tool

Lineage records the boolean node's tag, so `blank.cut(grooves, shaftBore, grub)`
gives all three tools one name and `generatedBy: "bore"` — the tool's own tag —
matches nothing at all. `timing-pulley.js` asked for `{ generatedBy: "machined",
curve: "circle", role: "hole", at: { z: "min" } }` and passed `expect({ count: 1
})` only because the twenty groove rims beside the bore rim were split into open
arcs by coplanar face splits, and `role: "hole"` wants a closed circle. The
backend now merges those faces (`unified`, in `backend.rs`), the arcs close, and
the same query matches 21 — the assertion had been resting on how fragmented the
topology happened to be. Cut in one tagged step per feature, as
`knurled-knob.js` does, and the name means something.

That merge moves counts elsewhere too, and the direction is always fewer: the
D-bore lead-in in `knurled-knob.js` used to select five arc fragments and now
selects the two curves they always described. A count over fragments is a count
over how the kernel happened to split a face. Assert over features.

### A cutter coplanar with the face it cuts loses the rim

`countersink()` first built its cone with the wide end exactly on the top face —
which is where a countersink geometrically ends. OCCT merged the cone's flat top
into that face, and the rim stopped being an edge the cut had generated: the
selector matched one edge instead of five, and no dimension looked wrong. The
helper now builds the cone 0.5 mm taller and wider along its own taper, so the
section at the face is still the called-out head diameter. Same rule as every
cutter in `examples/`: run past the material.

### A cut needs overlength at both ends, and the entry end is the silent one

Everything here — the examples, their comments, the entry above — states the
rule for where a cutter *exits*: run past the material, because a tool ending
exactly on the face it leaves through makes a zero-thickness sliver. Nothing
stated it for where a cutter *enters*, because until `display-bezel.js` every
seeded part cut through something and no cutter had an entry end to get wrong.
An external session recessing glass panels into a model car found the other
half: its cutters' outer faces were meant to lie on the body surface, and it
shipped a **0.004 mm** feather edge that `measure_wall_thickness` found
afterwards and nothing caught when it was made.

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

Read the bottom three rows as what they are. They are not a pocket with a thin
lid over it; **they are not a pocket at all.** The part is a solid block with a
sealed void inside, the front face is unbroken, the extra volume is the lid, and
the bounding box, the watertightness and `BRepCheck` are exactly what the
correct part reports. Face count is the only number that moves, and it moves
*up*.

**The microns come from arithmetic, not from typing them.** They cannot arise on
an axis-aligned face, which is why the trap needs a sloped one. Take a block
whose front face rises 20 mm over 85 mm, and a 4 mm panel recessed into it with
the recess's outer edge meant to lie on that face. Write the gradient the way a
sketch reads it — `0.235`, where the exact value is `20 / 85 = 0.23529…` — and
the outer edge runs *inside* the face by `(65 − x) · 0.000294 · cos 13.24°`:
0.0029 mm at one end of the cut and 0.019 mm at the other. OCCT builds it,
reports watertight, 11 faces, and removes exactly the cutter's own
4519.86 mm³ against the 4519.90 the parallelogram computes — the membrane
survives intact and nothing in the reply mentions it.

**And the measurement that finds it can miss it.** The field sweep behind
`measure_wall_thickness` samples the surface from seven rendered views, so it
reports a membrane only when a sample happens to land on one. On the sloped case
above, at its default 96 px it reported a minimum of 10.82 mm and *nothing*
below a 1 mm threshold; at 256 px it reported 0.0104 mm, which is the formula's
value at that point to six places. Raise `resolution` before believing a clean
answer, and see docs/PERCEPTION.md §5 for the other direction the number is
already known to be wrong in.

So the rule is one rule with two ends: **a cutter crosses every face it meets** —
past the material where it exits, proud of the material where it enters.
`holeFor` and `countersink` already do it at 0.5 mm; `examples/display-bezel.js`
is the part that does it on a recess, at the entry of its seat and at both ends
of its aperture.

### Nothing refuses a coincident cutter face, and the table above is why

Refusal is the stance elsewhere in this project, and the obvious reading of the
0.004 mm case is that the kernel knew the two planes were near-coincident and
returned an unmanufacturable solid rather than saying so. It does not refuse,
and after measuring it should not:

- **The case a refusal would have to fire on is not the coincident one.** Row 1
  of the table — exactly coplanar — is correct geometry, byte-for-byte the same
  result as standing the cutter proud, and it is what a boss trimmed back to a
  face or a slot cut flush with an underside produces. `examples/v-block.js`
  ships one: its strap slot's floor is exactly the block's own underside. A
  refusal on coincidence would refuse that.
- **What is wrong is *near*-coincidence, and it is a continuum.** 0.004 mm is an
  accident and 0.4 mm is a design; between them is every value, and any
  threshold is a number some part reaches legitimately. The kernel cannot see
  which one an author meant, because the difference is not in the geometry.
- **Measuring the outcome instead of the cause does not rescue it either — not
  today.** The wall sweep is cheap enough to run on every evaluation (10–40 ms
  at 96 px on these parts), but it reports **0.0055 mm** for
  `examples/hydraulic-line.js` and **0.0069 mm** for `examples/timing-pulley.js`,
  two shipped, correct parts. The hydraulic-line sample sits exactly on the seam
  where a bend's torus meets its straight run: the two fields are tangent there,
  so the sampled gradient is the wrong surface's normal and the ray it fires
  runs *along* the face instead of through the wall. That part's true minimum
  wall is 1.5 mm. Two false alarms in twenty parts is not a signal to put in
  front of an agent on every edit — it teaches the agent to ignore the line.

The honest shape of a fix is therefore neither a refusal nor a threshold. It is
to make the wall measurement trustworthy first — reject a sample whose normal is
not the surface's, which is what `thickness.rs` already claims to do and does not
— and only then to consider putting the measured minimum in the evaluation
reply, where it needs no threshold at all because it is a measurement rather
than a judgement.

### `role: "hole"` does not match a conical opening

A countersink rim is an inner boundary of the top face by any reading, and
`role: "hole"` drops it — `cover-plate.js` selects on `curve` and position
instead. Related to the D-bore case in docs/DSL_GAPS.md §6: the term matches a
narrower thing than its name suggests.

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

## A union of pieces that do not touch each other kills the fuse that joins them

`pipe()` builds a tube as runs and bend arcs and unions them. Assembled with
every arc first and the straight runs afterwards, OCCT **hung** on a two-bend
route and **segfaulted** on a three-bend one; assembled in path order — run,
arc, run, arc — the same route builds in milliseconds.

The reason is that the fold is pairwise. Arcs at different corners are disjoint
solids, so fusing them first produces a compound of separate lumps, and the
boolean that finally bridges them has to resolve every contact at once. Fusing
along the chain means each step touches what is already there.

**The rule:** when unioning a chain of shapes, union them in the order they
touch. This is cheap to get right and expensive to debug, because the failure
is a SIGSEGV three operations later.

## A coaxial torus groove in a cylinder segfaulted `UnifySameDomain` *(fixed)*

```js
cylinder(12, 14).cut(torus(11.5, 1))   // SIGSEGV, before OCCT 8.0.1
```

That is an O-ring groove, which is the shape a torus exists for. The **boolean
was fine** — the crash was in `unified()`, the `clean()` / `UnifySameDomain` pass
every result goes through to weld away imprint edges, and it died on the two
coaxial circular seams the cut leaves in the cylinder wall.

What did *not* crash, which is what made it identifiable — kept because it is
how the next crash of this family gets narrowed down:

| shape | result |
|---|---|
| `box(30, 30, 14).cut(torus(11.5, 1))` | fine |
| `cylinder(12, 14).cut(torus(20, 4))` — enters from the side | fine |
| `cylinder(12, 14).cut(cylinder(11, 20))` — coaxial, no torus | fine |
| `cylinder(12, 14).cut(torus(11.5, 1))` | **SIGSEGV** |

**Vendoring OCCT 8.0.1 fixed it**, and the `known_defect` marker came off
`eval/cases/torus-gland.json`, which is that marker working as designed rather
than a case being quietly relaxed. The case still runs, so the fix cannot
silently regress; `examples/hydraulic-line.js` now carries the groove in a real
part, which guards the same ground with a boolean under it.

What that part cannot do is a *catalogue* gland: a torus cut is a circle in
section, and a standard gland is rectangular and wider than the cord. The
blocker moved from the kernel to the section — docs/DSL_GAPS.md, "arcs in a
section".

# Changes from upstream occt-sys 0.2.0

LGPL-2.1, unchanged. Our own crates are MIT OR Apache-2.0; this one is not, and
modifications to it stay under its licence.

**The OCCT this ships is 8.0.1, not the 7.7.1 the upstream crate shipped.**

## Why vendor at all

This crate is the OpenCASCADE C++ kernel itself — `OCCT/` is source that cmake
compiles from scratch, not a prebuilt library. Three separate things needed it
in the tree rather than in the cargo registry cache:

1. **Patching OCCT.** A cargo registry checkout is checksum-verified and may be
   re-extracted at any time; edits to it are neither durable nor reviewable.
2. **Upgrading OCCT.** The published crates cannot do it. `opencascade-sys` was
   last released in August 2023 at 0.2.0 and pins `occt-sys = 0.2` (OCCT 7.7.1);
   `occt-sys` itself reached 0.6.0 (OCCT 7.8.1) but no binding crate consumes
   it. There is no version number to bump.
3. **Diagnosing anything.** `BRepCheck` was unbound, so there was no way to ask
   OpenCASCADE whether a shape it reported `IsDone()` on was actually valid. See
   `vendor/opencascade-sys/PARCAD-CHANGES.md`.

## How to patch OCCT

**`OCCT/` is a pristine upstream export and stays that way.** Changes go in
`patches/` as numbered diffs; `build.rs` copies the tree into `OUT_DIR` and
applies them there. See `patches/README.md` for the procedure.

This is the ordinary way to carry local changes to a vendored C++ dependency —
Debian, Buildroot, Yocto and Nixpkgs all do it — and it is what makes an upgrade
tractable: drop in the new tag, rebuild, and any patch that no longer fits fails
the build by name instead of disappearing into a tree that can no longer be
diffed against upstream.

OpenCASCADE used to ship its own version of this, `BUILD_PATCH`, which upstream
`occt-sys` relied on. It was removed in 7.9 — present in 7.7.1 and 7.8.1, gone
from `occt_toolkit.cmake` in 7.9.0 and from `CMakeLists.txt` by 7.9.3. The
staging step replaces it and does not depend on the OCCT version.

Anything we change should be offered upstream at
<https://github.com/Open-Cascade-SAS/OCCT>.

## The 8.0.1 upgrade

Imported from the official `V8_0_1` tag. Four things had to change, none of
them ours to argue with:

- **`src/` is now modular.** Packages moved from `src/<Package>` to
  `src/<Module>/<Toolkit>/<Package>` — `ChFi3d` is at
  `src/ModelingAlgorithms/TKFillet/ChFi3d`.
- **Data exchange runs through XCAF.** `TKDESTEP` and `TKDESTL` — which
  replaced `TKSTEP`/`TKSTEPAttr`/`TKSTEPBase`/`TKSTEP209` and `TKSTL` — now
  depend on `TKXCAF`, which depends on `TKV3d` and `TKService`. STEP and STL are
  not optional for parcad, so the `Visualization` and `ApplicationFramework`
  modules can no longer be trimmed out of the build. OCCT 7.7 had no such
  dependency, which is why upstream `occt-sys` could strip both and why its
  `patch/` directory existed at all.
- **`patch/` is deleted.** Everything in it was build configuration — trimmed
  `MODULES`/`PACKAGES`/`FILES` lists plus two empty stub headers — and its whole
  purpose was trimming the visualisation toolkits out of a 7.7 build. No
  algorithm code was ever in there. Replaced by `patches/`, which is for
  algorithm changes and nothing else.
- **`build.rs` stages and patches** instead of passing `BUILD_PATCH`, and
  disables the Draw module by flag.

## Deleted from the imported tree

Only non-source directories, none of which affect the build: `tests/` (75 MB),
`data/` (54 MB), `dox/` (16 MB), `.github/` and `samples/`. Everything under
`src/` is present and unmodified, including modules we do not compile — Draw is
switched off with `BUILD_MODULE_Draw=FALSE` rather than deleted, so that
`src/MODULES.cmake` needs no edit and the tree stays byte-identical to upstream.

The `exclude` list in `Cargo.toml` is a packaging directive for crates.io and
has no effect on a path dependency. It is left alone, and is now stale.

## Changes to OCCT itself

None. `patches/` is empty; see `patches/README.md`.

One patch has lived here and been deleted: `0001-debug-chfi3d-corners.patch`,
a trace carrying no fix. It answered its question — a blend that has to end on
a face it is tangent to is handled by `PerformExtremity` → `PerformOneCorner` →
`PerformIntersectionAtEnd` → `PerformMoreThreeCorner`, and not by
`ChFi3d_ExtendSurface`, which is never reached.

**No fix followed, deliberately.** The tangent configuration has no
non-degenerate answer to build: the fillet's width falls to zero at the
tangency, so the correct torus face has a boundary that touches itself at a
point, and `BRepCheck_SelfIntersectingWire` accurately describes the right
answer rather than reporting a wrong one. Fusion 360 does not solve it either —
its version of the part carries 7.6e-5 mm of clearance and never poses the
question. Teaching `PerformIntersectionAtEnd` to emit that face would buy a
shape nothing downstream can use.

parcad refuses the configuration instead, at the operation that produced it, and
keeps OCCT unmodified. Reasoning and measurements are in `docs/GOTCHAS.md`, "A
blend that ends on a face it is tangent to"; the behaviour is pinned by
`eval/cases/refuse-tangent-blend.json`,
`eval/cases/refuse-tangent-blend-in-bounds.json` and their control
`eval/cases/blend-runs-off-the-edge.json`.

Nothing was reported upstream, because there is no defect to report: the input
is geometrically degenerate, and the open "fillet returns a faulty shape" family
(#172, #691, #692, #694, #736, #899, #900, #1177, #1371, #1427, #1430) is
unowned and would not be advanced by another instance. What would be worth
offering is a way for OCCT to *refuse* this instead of returning
`IsDone() == true` — an API-contract change, not a patch we carry.

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

Two patches.

`patches/0002-unify-merge-must-not-abort.patch`: edge unification in
`ShapeUpgrade_UnifySameDomain` no longer aborts wholesale when a single
chain cannot build its union edge — the reachable case being a chain through
an edge that lies along a cylinder's parametric seam, whose two pcurves a
concatenation cannot both join. Found when parcad's `clean()` pass gained
real merge tolerances (see `vendor/opencascade/PARCAD-CHANGES.md`) and the
retainer's seam-side tangent generator became a merge candidate: the merge
threw "Courbes non jointives" and killed an otherwise valid build. The chain
is now left split instead. On the retainer the throw is no longer reachable —
`clean()` heals the stale dual representation first and the chain merges —
so the patch remains as hardening against the next such chain.

`patches/0001-tangent-pinch-corner.patch`: a corner treatment for
fillet spines that end on an exact tangency, `ChFi3d_Builder::PerformTangentPinch`,
dispatched from `PerformFilletOnVertex` ahead of the generic corner code. The
patch header carries the full account — what it fixes, how it was measured, and
its upstream status; the behaviour is held down by `eval/cases/tangent-blend.json`,
`eval/cases/tangent-blend-retainer.json` and their control
`eval/cases/blend-runs-off-the-edge.json`, and the history of the defect is in
docs/GOTCHAS.md, "A blend that ends on a face it is tangent to".

Two earlier conclusions recorded here did not survive contact with the fix, and
are corrected rather than erased:

- **"The tangent configuration has no non-degenerate answer to build" was
  wrong.** The fillet's width does fall to zero at the tangency, but the
  correct topology is not a face whose wire touches itself: it is the toroidal
  face trimmed by the grazing plane, ending at a vertex on its inner contact
  circle — an ordinary vertex where the trim curve and the contact circle meet
  at a finite angle. Where the spine passes through the tangency the two
  crescents share that apex; where it stops there, the extended grazing face
  and the support meet along the tangent generator, an edge like any other.
  Fusion 360's own export is this exact construction, torus trimmed by planes.

- **An earlier debug patch, `0001-debug-chfi3d-corners.patch`, was deleted**
  after establishing (on OCCT 7.7.1) the path `PerformExtremity` →
  `PerformOneCorner` → `PerformIntersectionAtEnd` → `PerformMoreThreeCorner`.
  On 8.0.1 that route is real for the single-stripe form (the retainer);
  the two-stripe form reaches `PerformMoreThreeCorner` directly from
  `PerformFilletOnVertex`. Both end in the same GeomPlate corner cap, which is
  what the fix replaces. `ChFi3d_ExtendSurface` remains not involved.

What has not changed: OCCT still answers `IsDone() == true` while handing back
broken geometry for fillet failures outside this configuration — the open
"fillet returns a faulty shape" family (#172, #691, #692, #694, #736, #899,
#900, #1177, #1371, #1427, #1430). parcad's own gates (`check_blend`, the
worker's watertight backstop) stay, because they do not depend on knowing why a
shape is wrong. The patch has not yet been offered upstream; it should be.

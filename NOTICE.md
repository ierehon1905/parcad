# Licensing

Not everything in this tree is under the same licence. This file says which is
which, because the difference matters if you redistribute a binary.

## Our own code — MIT OR Apache-2.0

`crates/`, `app/` (both the Rust host and the TypeScript frontend), `tools/`,
`field/`, `examples/`, `eval/`, `cmake/` and `docs/` — including the screenshots
in `docs/images/`, which are this application rendering its own examples — at
your option under either:

- [LICENSE-MIT](LICENSE-MIT) — MIT
- [LICENSE-APACHE](LICENSE-APACHE) — Apache License 2.0

Unless you state otherwise, a contribution you intentionally submit for
inclusion is dual-licensed the same way, with no additional terms.

## `vendor/opencascade` — LGPL-2.1

A fork of the crates.io [`opencascade`](https://github.com/bschwind/opencascade-rs)
0.2.0 crate by Brian Schwind, kept minimal so it can go upstream. It is
**LGPL-2.1** ([vendor/opencascade/LICENSE](vendor/opencascade/LICENSE)) and our
modifications to it stay under that licence — see
[PARCAD-CHANGES.md](vendor/opencascade/PARCAD-CHANGES.md) for every one of them.

Do not move code between this directory and `crates/`; the licences differ.

## OpenCASCADE itself — LGPL-2.1 with an exception

**Vendored in full**, at [vendor/occt-sys/OCCT](vendor/occt-sys/OCCT): OCCT
8.0.1, imported from the upstream `V8_0_1` tag, about 15,800 files and 144 MB.
Nothing is fetched at build time — `vendor/occt-sys/build.rs` stages this tree
into `OUT_DIR`, applies the two local patches in `vendor/occt-sys/patches/`, and
runs cmake on the result. Those patches are themselves LGPL-2.1, and
[PARCAD-CHANGES.md](vendor/occt-sys/PARCAD-CHANGES.md) records what was imported
and what was dropped.

OCCT is **LGPL-2.1 with the Open CASCADE exception**, both texts in-tree:

- [LICENSE_LGPL_21.txt](vendor/occt-sys/OCCT/LICENSE_LGPL_21.txt)
- [OCCT_LGPL_EXCEPTION.txt](vendor/occt-sys/OCCT/OCCT_LGPL_EXCEPTION.txt)

The exception lifts the LGPL's treatment of material inlined from header files,
on one condition: a prominent notice in the supporting documentation saying that
the software makes use of facilities provided by Open CASCADE Technology. This
file and [README.md](README.md) both carry it. The exception does **not** waive
the relinking obligation for linking the library itself.

`OCCT/src/FoundationClasses/TKernel/Standard_Strtod.cxx` carries a separate
permissive David M. Gay / Lucent notice, retained as it stands: not every file
under `OCCT/` is LGPL-2.1.

## `vendor/occt-sys` and `vendor/opencascade-sys` — LGPL-2.1

Forks of the `occt-sys` and `opencascade-sys` crates from the same
[opencascade-rs](https://github.com/bschwind/opencascade-rs) repository, both
**LGPL-2.1**, each with its own `PARCAD-CHANGES.md`.

Note what that implies, because it is easy to miss: ParCAD-authored C++ lives in
`vendor/opencascade-sys/include/wrapper.hxx` — `Shape_geometry_json`,
`Shape_topology_report`, `BRepCheck_report` and others — and is LGPL-2.1 like the
crate it extends. That is the correct direction for a contribution to an LGPL
work, but it means those parts are not relicensable as MIT/Apache without being
reimplemented.

Do not move code between any of these directories and `crates/`; the licences
differ.

## `fidget` — MPL-2.0

The SDF kernel. [fidget](https://github.com/mkeeter/fidget) 0.5.0 is a direct,
non-optional dependency of `parcad-core`, so unlike the OCCT chain it is in
**every** ParCAD binary — the app, the CLI and the eval harness alike. MPL-2.0 is
file-level copyleft: it does not reach our own code, but distributing a binary
carries an obligation to make the source of the MPL files available. It is used
unmodified from crates.io, so naming the upstream and the version discharges it.

`dynasm` and `dynasmrt` arrive with fidget, and `option-ext`, `cssparser`,
`selectors` and `dtoa-short` arrive through Tauri, all MPL-2.0 on the same terms.
No dependency of this project is GPL, AGPL or SSPL.

## What this means for a binary you ship

Building and running from source is unencumbered. Redistributing a **binary** is
what triggers obligations, and there are three:

- **LGPL-2.1 §6, relinking.** OCCT is built into `parcad-occt-worker`. Anyone who
  gets that binary must be able to use their own build of OCCT instead. The next
  section says how.
- **MPL-2.0 §3.2**, source availability for fidget and the crates above.
- **The licence texts themselves** must accompany the binary. `bundle.resources`
  in `app/src-tauri/tauri.conf.json` ships this file, both of ours, and both of
  OCCT's into the bundle's `Resources/`.

## How to use your own OpenCASCADE

The LGPL says you must be able to replace the OpenCASCADE inside this program
with your own version. Here is how.

OpenCASCADE is not inside the app. It is inside one separate file called
`parcad-occt-worker`. The app talks to that file and nothing else in the app
contains OpenCASCADE code. So you only have to rebuild that one file.

**1. Get the source.** All of it is public, including the copy of OpenCASCADE
this project builds: <https://github.com/ierehon1905/parcad>

**2. Change OpenCASCADE if you want to.** It is in `vendor/occt-sys/OCCT/`. You
can edit it, or replace it with a different version.

**3. Build a new worker.**

```bash
tools/build-worker.sh
```

This compiles OpenCASCADE the first time — about five minutes on a fast
machine, considerably longer on few cores.
If you already have an OpenCASCADE build, you can point at it instead and skip
that:

```bash
PARCAD_OCCT_PREBUILT=/path/to/occt-install tools/build-worker.sh
```

**4. Tell the app to use your worker.**

```bash
PARCAD_OCCT_WORKER=/path/to/your/parcad-occt-worker
```

The app reads that variable and runs your file instead of the one it shipped
with. You do not have to modify the app, and you do not need our permission.

If any of this does not work for you, that is a bug in this project. Please open
an issue.

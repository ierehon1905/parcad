# Licensing

Not everything in this tree is under the same licence. This file says which is
which, because the difference matters if you redistribute a binary.

## Our own code — MIT OR Apache-2.0

`crates/`, `app/` (both the Rust host and the TypeScript frontend), `tools/`,
`examples/`, `eval/`, `cmake/` and `docs/`, at your option under either:

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

Not vendored here. `crates/parcad-occt`'s `kernel` feature builds it through
`opencascade-sys`, which fetches the upstream sources. OCCT is LGPL-2.1 plus an
additional exception; see <https://dev.opencascade.org/resources/licensing>.

Only the `parcad-occt-worker` binary links it. The application, the CLI and
everything else talk to that worker over a pipe, so they carry no OCCT code.

## What this means for a binary you ship

Building and running from source is unencumbered. Redistributing a **binary**
that statically links the LGPL parts is what triggers the LGPL's relinking
obligation — you have to let a recipient replace those parts. Because the OCCT
side is confined to one separate worker executable, that is a solvable problem
rather than a whole-application one, but it is a deliberate decision to make
before shipping, not an afterthought.

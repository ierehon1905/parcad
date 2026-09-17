//! ParCAD web's host: `parcad-host` compiled to WebAssembly.
//!
//! The tab's counterpart of the desktop host process. Its Web Worker
//! (`app/src/page/host-worker.ts`) calls in through the functions in
//! `exports.rs`, which only move bytes: what a call does is
//! `parcad_host::page`, over the same service, projects, session and MCP
//! server the desktop hosts. The kernel is the page's other module,
//! `crates/parcad-wasm`, reached through the page.

#[cfg(target_os = "emscripten")]
mod exports;

fn main() {}

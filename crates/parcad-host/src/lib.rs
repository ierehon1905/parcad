//! Everything parcad can do, and the hosts that expose it — with no window in
//! the crate at all.
//!
//! `service` owns every capability. `http` and `mcp` are adapters onto it, and
//! so is the desktop app's IPC layer in `app/src-tauri`, which depends on this
//! crate rather than the other way round. That direction is the point: the
//! headless `parcad serve` and the desktop window run the same router, the same
//! MCP server and the same project folder, and neither can grow a feature the
//! other lacks.
//!
//! A browser tab is the third host. The crate compiles to WebAssembly
//! (`crates/parcad-wasm-host`), with `page` as its transport in place of a
//! socket: the same routes, the same MCP server and the same project folder,
//! kept in the browser's storage.

pub mod assets;
pub mod docs;
/// Not in a browser tab: there is no editor there to hand declarations to, and
/// the page's filesystem is the page's own.
#[cfg(not(target_arch = "wasm32"))]
pub mod editor_types;
pub mod generative;
#[cfg(not(target_os = "emscripten"))]
pub mod http;
pub mod mcp;
pub mod page;
pub mod projects;
pub mod routes;
pub mod script;
pub mod service;
pub mod session;

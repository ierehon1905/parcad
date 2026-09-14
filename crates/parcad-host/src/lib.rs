//! Everything parcad can do, and the two hosts that expose it — with no window
//! in the crate at all.
//!
//! `service` owns every capability. `http` and `mcp` are adapters onto it, and
//! so is the desktop app's IPC layer in `app/src-tauri`, which depends on this
//! crate rather than the other way round. That direction is the point: the
//! headless `parcad serve` and the desktop window run the same router, the same
//! MCP server and the same project folder, and neither can grow a feature the
//! other lacks.

pub mod docs;
pub mod http;
pub mod mcp;
pub mod projects;
pub mod script;
pub mod service;
pub mod session;

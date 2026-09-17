//! The exact kernel in a browser tab.
//!
//! The tab's counterpart of the `parcad-occt-worker` process: one request
//! packet in, one reply packet out (`parcad_occt::packet`), over linear memory.
//! The page's kernel Web Worker (`app/src/page/kernel-worker.ts`) writes a
//! request with `parcad_alloc`, calls `parcad_call`, and hands the tagged reply
//! back with `parcad_free`. What a request *does* is the native worker's own
//! code, `parcad_occt::serve::run`; everything a host does with the answer —
//! measuring, rendering, the MCP tools — is `parcad-host`, compiled into the
//! page's other module (`crates/parcad-wasm-host`).
//!
//! What is not here is the process boundary. A native worker that crashes or
//! hangs is a child process the host replaces; in a tab it is a Web Worker the
//! page terminates and starts again, which is `kernel.ts`'s job.

#[cfg(target_os = "emscripten")]
mod kernel;

fn main() {}

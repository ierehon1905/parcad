//! The exact kernel in a browser tab.
//!
//! One request in, one reply out, over linear memory: the page's Web Worker
//! (`app/src/kernel-worker.ts`) writes a JSON request with `parcad_alloc`,
//! calls `parcad_call`, reads the tagged reply and hands it back with
//! `parcad_free`. What a request *does* is the native worker's own code
//! (`parcad_occt::serve::run`), and what an evaluation reply *is* is
//! `parcad_evaluation` — the same two definitions the desktop host serialises.
//!
//! What is not here is the process boundary. A native worker that crashes or
//! hangs is a child process the host replaces; in a tab it is a Web Worker the
//! page terminates and starts again, which is `kernel-worker.ts`'s job.

#[cfg(target_os = "emscripten")]
mod kernel;

fn main() {}

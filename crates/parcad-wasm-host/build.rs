//! How the host module is linked. Nothing happens on a native target.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("emscripten") {
        return;
    }
    for arg in [
        // A factory the host's Web Worker imports and calls.
        "-sMODULARIZE=1",
        "-sEXPORT_ES6=1",
        "-sEXPORT_NAME=createHost",
        "-sENVIRONMENT=worker",
        "-sINITIAL_MEMORY=32MB",
        "-sALLOW_MEMORY_GROWTH=1",
        "-sMAXIMUM_MEMORY=4GB",
        // QuickJS measures its own stack against this; see docs/GOTCHAS.md.
        "-sSTACK_SIZE=16MB",
        "-sEXPORTED_FUNCTIONS=_main,_host_alloc,_host_free,_host_start,_host_link,_host_viewer,_host_call,_host_retry,_host_poll,_host_forget,_host_answer,_host_events",
        "-sEXPORTED_RUNTIME_METHODS=HEAPU8,HEAPU32,FS",
        // The project folder is a filesystem the page persists to IndexedDB.
        "-sFORCE_FILESYSTEM=1",
        "-lidbfs.js",
    ] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}

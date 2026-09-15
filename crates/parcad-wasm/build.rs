//! How the browser module is linked. Nothing happens on a native target.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("emscripten") {
        return;
    }
    for arg in [
        // A factory the Web Worker imports and calls, rather than a script that
        // runs itself on load.
        "-sMODULARIZE=1",
        "-sEXPORT_ES6=1",
        "-sEXPORT_NAME=createKernel",
        "-sENVIRONMENT=worker",
        "-sDEFAULT_TO_CXX=1",
        "-sINITIAL_MEMORY=64MB",
        "-sALLOW_MEMORY_GROWTH=1",
        "-sMAXIMUM_MEMORY=4GB",
        "-sSTACK_SIZE=16MB",
        // main() is empty; the page calls in through these.
        "-sEXPORTED_FUNCTIONS=_main,_parcad_alloc,_parcad_call,_parcad_free",
        "-sEXPORTED_RUNTIME_METHODS=HEAPU8,HEAPU32",
        // STEP is written by OCCT to a path; MEMFS gives it one in memory.
        "-sFORCE_FILESYSTEM=1",
    ] {
        println!("cargo:rustc-link-arg-bins={arg}");
    }
}

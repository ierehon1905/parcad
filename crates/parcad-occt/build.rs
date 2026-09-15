//! Record the target triple the host half was compiled for, and link the
//! worker when that target is WebAssembly.
//!
//! `host::worker_path()` needs the triple to recognise a Tauri sidecar, which is
//! installed under a triple-suffixed name. Cargo hands the triple to a build
//! script and to nothing else, so this script is the only way to have it as a
//! constant instead of guessing from `std::env::consts`, which knows the OS and
//! the architecture but never the whole triple.
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").expect("cargo always sets TARGET for a build script");
    println!("cargo::rustc-env=PARCAD_TARGET_TRIPLE={target}");

    if !target.contains("emscripten") {
        return;
    }
    // The worker under Node, for the eval corpus (playground/node-worker.sh).
    // NODERAWFS hands it the real stdin, stderr and reply files the host's
    // protocol runs on; DEFAULT_TO_CXX because rustc links with emcc, which
    // otherwise leaves libc++ out. OpenCASCADE recurses deeply, hence the stack.
    for arg in [
        "-sDEFAULT_TO_CXX=1",
        "-sNODERAWFS=1",
        "-sENVIRONMENT=node",
        "-sINITIAL_MEMORY=64MB",
        "-sALLOW_MEMORY_GROWTH=1",
        "-sMAXIMUM_MEMORY=4GB",
        "-sSTACK_SIZE=16MB",
        "-sEXIT_RUNTIME=1",
    ] {
        println!("cargo::rustc-link-arg-bin=parcad-occt-worker={arg}");
    }
}

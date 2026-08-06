//! Record the target triple the host half was compiled for.
//!
//! `host::worker_path()` needs it to recognise a Tauri sidecar, which is
//! installed under a triple-suffixed name. Cargo hands the triple to a build
//! script and to nothing else, so this three-line script is the only way to
//! have it as a constant instead of guessing from `std::env::consts`, which
//! knows the OS and the architecture but never the whole triple.
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!(
        "cargo::rustc-env=PARCAD_TARGET_TRIPLE={}",
        std::env::var("TARGET").expect("cargo always sets TARGET for a build script")
    );
}

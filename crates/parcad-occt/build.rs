//! Record what the host half was compiled for and from, and link the worker
//! when that target is WebAssembly.
//!
//! `host::worker_path()` needs the triple to recognise a Tauri sidecar, which is
//! installed under a triple-suffixed name. Cargo hands the triple to a build
//! script and to nothing else, so this script is the only way to have it as a
//! constant instead of guessing from `std::env::consts`, which knows the OS and
//! the architecture but never the whole triple.
//!
//! The profile and the source digest are the build identity a host sends its
//! worker with every request (`protocol::BuildId`): the host is compiled
//! without the `kernel` feature and the worker with it, so they are separate
//! compilations that nothing else keeps in step.
use std::path::{Path, PathBuf};

/// What the worker's behaviour is compiled from, relative to this crate. The
/// OpenCASCADE install is not here: `occt-sys` owns that, and
/// `PARCAD_OCCT_PREBUILT` is the caller's own assertion about it.
const SOURCES: &[&str] = &[
    "src",
    "Cargo.toml",
    "../parcad-core/src",
    "../parcad-core/Cargo.toml",
    "../../vendor/opencascade/src",
    "../../vendor/opencascade/include",
    "../../vendor/opencascade/build.rs",
    "../../vendor/opencascade/Cargo.toml",
    "../../vendor/opencascade-sys/src",
    "../../vendor/opencascade-sys/include",
    "../../vendor/opencascade-sys/build.rs",
    "../../vendor/opencascade-sys/Cargo.toml",
];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").expect("cargo always sets TARGET for a build script");
    println!("cargo::rustc-env=PARCAD_TARGET_TRIPLE={target}");
    println!("cargo::rustc-env=PARCAD_BUILD_PROFILE={}", profile());
    println!("cargo::rustc-env=PARCAD_SOURCE_DIGEST={:016x}", source_digest());

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

/// The cargo profile's name, which cargo gives a build script only as the
/// directory it builds in: `<target>/[<triple>/]<profile>/build/<pkg>-<hash>/out`.
/// Its `PROFILE` variable says `release` for `iterate` too.
fn profile() -> String {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo always sets OUT_DIR"));
    let name = out
        .ancestors()
        .nth(3)
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    assert!(
        matches!(name.as_str(), "debug" | "release" | "iterate"),
        "cannot tell the cargo profile from OUT_DIR={} (read {name:?}). Cargo's build \
         directory layout has changed; update profile() in crates/parcad-occt/build.rs",
        out.display()
    );
    name
}

/// FNV-1a over every file in `SOURCES`, by relative path and contents, in a
/// fixed order, so every checkout of the same tree agrees.
fn source_digest() -> u64 {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for source in SOURCES {
        println!("cargo::rerun-if-changed={source}");
        collect(&manifest.join(source), &mut files);
    }
    files.sort();
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            digest ^= u64::from(*byte);
            digest = digest.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for file in files {
        let relative = file.strip_prefix(&manifest).expect("every source is under the manifest");
        eat(relative.to_string_lossy().as_bytes());
        eat(&[0]);
        eat(&std::fs::read(&file).unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display())));
        eat(&[0]);
    }
    digest
}

/// Dotfiles are left out: an editor's swap file or a `.DS_Store` is not source.
fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
        return;
    }
    if path.is_dir() {
        let entries = std::fs::read_dir(path).unwrap_or_else(|e| panic!("cannot list {}: {e}", path.display()));
        for entry in entries {
            collect(&entry.expect("a directory entry").path(), files);
        }
    } else if path.is_file() {
        files.push(path.to_path_buf());
    }
}

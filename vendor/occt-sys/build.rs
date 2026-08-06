//! Build OpenCASCADE from a pristine upstream tree plus our patch series.
//!
//! `OCCT/` is an untouched export of an upstream tag, so upgrading is a matter
//! of swapping the directory. Our changes live as numbered diffs in `patches/`
//! and are applied to a staged copy under `OUT_DIR`, never to `OCCT/` itself —
//! which is what keeps "what is ours" and "what is theirs" separable after an
//! upgrade. See PARCAD-CHANGES.md.
//!
//! OpenCASCADE had its own mechanism for this, `BUILD_PATCH`, and removed it in
//! 7.9. This replaces it and does not depend on the OCCT version.

use std::path::{Path, PathBuf};

const LIB_DIR: &str = "lib";
const INCLUDE_DIR: &str = "include";

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let staged = out.join("occt-src");

    let pristine = manifest.join("OCCT");
    let patches = collect_patches(&manifest.join("patches"));
    let targets = patch_targets(&patches);

    // Mirror upstream, then overlay the patched files separately. Doing it in
    // two passes is what lets a rebuild with an unchanged patch series touch
    // nothing at all: if the mirror wrote pristine content over a patched file
    // and `patch` then rewrote it, that file's mtime would move every build and
    // its translation unit would recompile every build.
    stage_sources(&pristine, &staged, &targets);
    overlay_patched(&pristine, &staged, &out.join("occt-patched"), &patches, &targets);

    println!("cargo:rerun-if-changed=OCCT");
    println!("cargo:rerun-if-changed=patches");

    let dst = cmake::Config::new(&staged)
        .define("BUILD_LIBRARY_TYPE", "Static")
        // Draw is the Tcl test harness and nothing links it. Disabled by flag
        // rather than by deleting it, so `OCCT/` stays a pristine export.
        //
        // Visualization and ApplicationFramework cannot be disabled: OCCT 8.0
        // routed data exchange through XCAF, so TKDESTEP and TKDESTL depend on
        // TKXCAF, which depends on TKV3d and TKService. STEP and STL are not
        // optional for parcad, so neither is that chain.
        .define("BUILD_MODULE_Draw", "FALSE")
        .define("USE_D3D", "FALSE")
        .define("USE_DRACO", "FALSE")
        .define("USE_EIGEN", "FALSE")
        .define("USE_FFMPEG", "FALSE")
        .define("USE_FREEIMAGE", "FALSE")
        .define("USE_FREETYPE", "FALSE")
        .define("USE_GLES2", "FALSE")
        .define("USE_OPENGL", "FALSE")
        .define("USE_OPENVR", "FALSE")
        .define("USE_RAPIDJSON", "FALSE")
        .define("USE_TBB", "FALSE")
        .define("USE_TCL", "FALSE")
        .define("USE_TK", "FALSE")
        .define("USE_VTK", "FALSE")
        .define("USE_XLIB", "FALSE")
        .define("INSTALL_DIR_LIB", LIB_DIR)
        .define("INSTALL_DIR_INCLUDE", INCLUDE_DIR)
        .build();

    println!(
        "cargo:rustc-env=OCCT_LIB_PATH={}",
        dst.join(LIB_DIR).to_str().expect("path is valid Unicode")
    );
    println!(
        "cargo:rustc-env=OCCT_INCLUDE_PATH={}",
        dst.join(INCLUDE_DIR).to_str().expect("path is valid Unicode")
    );
}

/// Every `*.patch` in `patches/`, in filename order.
///
/// The numeric prefix is the apply order, so a later patch may depend on an
/// earlier one having landed.
fn collect_patches(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "patch"))
        .collect();
    found.sort();
    found
}

/// Mirror the pristine tree into `OUT_DIR` so patches have somewhere to land.
///
/// Only writes files whose contents actually differ. That matters more than it
/// sounds: `make` decides what to recompile from mtimes, so a blind re-copy
/// makes every one of OpenCASCADE's ~14 000 sources look new and turns a
/// one-line kernel edit into a five-minute rebuild. Leaving identical files
/// untouched keeps their mtimes and reduces that to the translation units the
/// patch series actually changes.
fn stage_sources(src: &Path, staged: &Path, skip: &std::collections::HashSet<PathBuf>) {
    mirror(src, staged, skip, Path::new(""));
}

/// Every path a patch writes to, as a tree-relative path, read from its
/// `+++ b/<path>` headers.
fn patch_targets(patches: &[PathBuf]) -> std::collections::HashSet<PathBuf> {
    let mut targets = std::collections::HashSet::new();
    for patch in patches {
        let text = std::fs::read_to_string(patch)
            .unwrap_or_else(|e| panic!("reading patch {}: {e}", patch.display()));
        for line in text.lines() {
            let Some(rest) = line.strip_prefix("+++ ") else {
                continue;
            };
            // "+++ b/src/Foo/Bar.cxx\t2026-01-01" — drop the -p1 prefix and any
            // trailing timestamp.
            let path = rest.split('\t').next().unwrap_or(rest).trim();
            if let Some((_, stripped)) = path.split_once('/') {
                targets.insert(PathBuf::from(stripped));
            }
        }
    }
    targets
}

/// Rebuild the patched files from pristine sources in a scratch tree, then move
/// only genuinely-changed results into the staged tree.
///
/// The scratch tree is wiped each run: `patch` refuses an already-applied diff,
/// so every application has to start from pristine content.
fn overlay_patched(
    pristine: &Path,
    staged: &Path,
    scratch: &Path,
    patches: &[PathBuf],
    targets: &std::collections::HashSet<PathBuf>,
) {
    if patches.is_empty() {
        return;
    }
    if scratch.exists() {
        std::fs::remove_dir_all(scratch)
            .unwrap_or_else(|e| panic!("clearing {}: {e}", scratch.display()));
    }
    for rel in targets {
        let from = pristine.join(rel);
        if !from.exists() {
            continue; // a patch that creates a new file has nothing to seed
        }
        let to = scratch.join(rel);
        std::fs::create_dir_all(to.parent().expect("target has a parent"))
            .unwrap_or_else(|e| panic!("creating {}: {e}", to.display()));
        std::fs::copy(&from, &to)
            .unwrap_or_else(|e| panic!("copying {}: {e}", from.display()));
    }
    for patch in patches {
        apply(patch, scratch);
    }
    for rel in targets {
        let from = scratch.join(rel);
        if !from.exists() {
            continue;
        }
        let to = staged.join(rel);
        std::fs::create_dir_all(to.parent().expect("target has a parent"))
            .unwrap_or_else(|e| panic!("creating {}: {e}", to.display()));
        write_if_changed(&from, &to);
    }
}

fn mirror(
    src: &Path,
    dst: &Path,
    skip: &std::collections::HashSet<PathBuf>,
    rel: &Path,
) {
    std::fs::create_dir_all(dst).unwrap_or_else(|e| panic!("creating {}: {e}", dst.display()));

    let mut expected = std::collections::HashSet::new();
    let entries =
        std::fs::read_dir(src).unwrap_or_else(|e| panic!("reading {}: {e}", src.display()));
    for entry in entries {
        let entry = entry.expect("directory entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let child = rel.join(entry.file_name());
        expected.insert(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            mirror(&from, &to, skip, &child);
        } else if !skip.contains(&child) {
            write_if_changed(&from, &to);
        }
    }

    // Drop anything the upstream tree no longer has, so an OCCT upgrade that
    // deletes a file does not leave it behind to be compiled.
    if let Ok(existing) = std::fs::read_dir(dst) {
        for entry in existing.filter_map(Result::ok) {
            if expected.contains(&entry.file_name()) {
                continue;
            }
            let stale = entry.path();
            let removed = if entry.file_type().is_ok_and(|t| t.is_dir()) {
                std::fs::remove_dir_all(&stale)
            } else {
                std::fs::remove_file(&stale)
            };
            removed.unwrap_or_else(|e| panic!("removing stale {}: {e}", stale.display()));
        }
    }
}

/// Copy `from` over `to` only when the bytes differ, leaving the mtime alone
/// otherwise. Used for both the pristine mirror and the patched files.
fn write_if_changed(from: &Path, to: &Path) {
    // Same size and staged no older than the source means unchanged. This is
    // the same bet `make` makes, and it is worth making: the alternative is
    // reading 137 MB of OpenCASCADE on every build script run.
    if let (Ok(src), Ok(dst)) = (from.metadata(), to.metadata()) {
        if src.len() == dst.len() {
            if let (Ok(src_t), Ok(dst_t)) = (src.modified(), dst.modified()) {
                if dst_t >= src_t {
                    return;
                }
            }
        }
    }
    let wanted =
        std::fs::read(from).unwrap_or_else(|e| panic!("reading {}: {e}", from.display()));
    if std::fs::read(to).is_ok_and(|current| current == wanted) {
        return;
    }
    std::fs::write(to, &wanted)
        .unwrap_or_else(|e| panic!("writing {}: {e}", to.display()));
}

/// Apply one patch to the staged tree with `patch -p1`.
///
/// A rejected patch is a hard error: it almost always means the OCCT tree was
/// upgraded underneath a change that no longer fits, and continuing would build
/// a kernel silently missing a fix parcad depends on.
fn apply(patch: &Path, staged: &Path) {
    let name = patch.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let file = std::fs::File::open(patch)
        .unwrap_or_else(|e| panic!("opening patch {}: {e}", patch.display()));
    let status = std::process::Command::new("patch")
        .current_dir(staged)
        .args(["-p1", "--forward", "--silent"])
        .stdin(file)
        .status()
        .unwrap_or_else(|e| {
            panic!("running `patch` for {name}: {e}\n  `patch` must be on PATH to build OCCT.")
        });
    if !status.success() {
        panic!(
            "patch {name} did not apply to the OCCT tree.\n  \
             This usually means OCCT/ was upgraded and the patch needs rebasing.\n  \
             Fix: apply it by hand against vendor/occt-sys/OCCT, regenerate it with\n  \
             `diff -ru` into vendor/occt-sys/patches/{name}, and record the rebase in\n  \
             vendor/occt-sys/PARCAD-CHANGES.md."
        );
    }
    println!("cargo:warning=occt-sys: applied {name}");
}

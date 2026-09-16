use std::path::{Path, PathBuf};

/// Embed the built frontend, so `parcad serve` carries the UI the way the
/// desktop app does.
///
/// `app/dist` is what `cd app && bun run build` writes, and it is the same
/// bundle Tauri embeds, so the two hosts cannot serve different builds. It is
/// also gitignored and absent on a fresh clone: a binary built before the
/// frontend hosts the API and MCP and says at `/` what to build. That is a
/// warning here and a message there rather than a failed build, because
/// `cargo build --locked --release` is documented to need no frontend.
fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist = manifest.join("../../app/dist");
    // A watched path that does not exist is stale on every build, which
    // recompiled this crate with full LTO each time; an empty dist is watched
    // like a full one and still changes when the frontend is built into it.
    if !dist.exists() {
        std::fs::create_dir_all(&dist).expect("creating app/dist for cargo to watch");
    }
    println!("cargo:rerun-if-changed={}", dist.display());

    let mut files = Vec::new();
    if dist.is_dir() {
        collect(&dist, &dist, &mut files);
    }
    if files.is_empty() {
        println!(
            "cargo:warning=app/dist is missing, so this `parcad serve` will host the API \
             and MCP but no UI. Build the frontend first: cd app && bun run build"
        );
    }

    let mut source = String::from("pub const UI: &[(&str, &[u8])] = &[\n");
    for (relative, absolute) in &files {
        source.push_str(&format!(
            "    ({:?}, include_bytes!({:?})),\n",
            relative,
            absolute.to_string_lossy()
        ));
    }
    source.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("ui.rs");
    std::fs::write(out, source).expect("writing the embedded frontend table");
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(root, &path, files);
            continue;
        }
        // Source maps are for a developer's browser and would double the
        // binary; nothing in the app requests one.
        if path.extension().is_some_and(|e| e == "map") {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        files.push((relative, path));
    }
}

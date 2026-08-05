use std::path::PathBuf;

fn main() {
    tauri_build::build();
    bundle_dsl();
}

/// Compile `app/src/dsl.ts` into one plain-JS file for the script sandbox.
///
/// The sandbox runs agent-authored scripts against the *same* authoring layer
/// the editor and `tools/run.ts` use. Bundling it here rather than checking a
/// built copy into the tree is the same rule as the intent graphs: a generated
/// artifact that is committed does not fail when its source changes under it,
/// it just quietly describes an older DSL. This one would be worse than stale
/// documentation — an agent would be told an operation exists that the kernel
/// no longer has, or the reverse.
fn bundle_dsl() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let app_src = manifest.join("../src");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // The DSL's own imports are followed by the bundler; these are the files it
    // reaches today, and a new one shows up as a stale build rather than a
    // silent miss only if it is listed. Watch the directory's two current
    // entry points explicitly.
    for file in ["dsl.ts", "selectors.ts"] {
        println!("cargo:rerun-if-changed={}", app_src.join(file).display());
    }

    // An IIFE rather than a module: the sandbox evaluates one script and then
    // reads one global, with no module resolver to implement and no loader to
    // accidentally give a script filesystem reach.
    let entry = out.join("dsl-entry.js");
    std::fs::write(
        &entry,
        format!(
            "import * as dsl from {};\nglobalThis.__parcadDsl = dsl;\n",
            serde_json::to_string(&app_src.join("dsl.ts").to_string_lossy().to_string()).unwrap()
        ),
    )
    .expect("writing the DSL bundle entry point");

    let bundle = out.join("dsl.bundle.js");
    let status = std::process::Command::new("bun")
        .arg("build")
        .arg(&entry)
        .arg("--format=iife")
        .arg("--target=browser")
        .arg("--outfile")
        .arg(&bundle)
        .status();

    match status {
        Ok(status) if status.success() => {}
        Ok(status) => panic!(
            "bundling app/src/dsl.ts for the script sandbox failed ({status}).\n\
             Run it by hand to see why:\n  \
             bun build app/src/dsl.ts --format=iife --target=browser --outfile /tmp/dsl.js"
        ),
        Err(e) => panic!(
            "could not run `bun` to bundle app/src/dsl.ts for the script sandbox: {e}\n\
             The app already needs bun to build its frontend; install it from https://bun.sh \
             and re-run the build."
        ),
    }
}

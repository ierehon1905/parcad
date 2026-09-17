/// Tauri's build step, minus the kernel sidecar in a dev build: tauri-build
/// copies it beside the binary, where a dev host needs a worker of its own
/// profile, and a fresh clone has none to copy (docs/GOTCHAS.md).
fn main() {
    if std::env::var("DEP_TAURI_DEV").as_deref() == Ok("true") {
        let patch = match std::env::var("TAURI_CONFIG") {
            Ok(given) => serde_json::from_str(&given).ok(),
            Err(_) => Some(serde_json::json!({})),
        };
        // A TAURI_CONFIG that is not a JSON object is left for tauri-build to refuse.
        if let Some(mut patch @ serde_json::Value::Object(_)) = patch {
            patch["bundle"]["externalBin"] = serde_json::Value::Null;
            std::env::set_var("TAURI_CONFIG", patch.to_string());
        }
    }
    tauri_build::build();
}

//! The frontend `parcad serve` carries: `app/dist`, embedded by `build.rs`.

use parcad_host::http::{Asset, Assets};

include!(concat!(env!("OUT_DIR"), "/ui.rs"));

pub fn carries_ui() -> bool {
    !UI.is_empty()
}

pub struct Embedded;

impl Assets for Embedded {
    fn get(&self, path: &str) -> Option<Asset> {
        let (_, bytes) = UI.iter().find(|(name, _)| *name == path)?;
        Some(Asset {
            mime_type: mime_type(path).to_string(),
            bytes: bytes.to_vec(),
        })
    }

    fn how_to_embed(&self) -> String {
        "This `parcad` was built before the frontend. Build it and rebuild the CLI: \
         cd app && bun run build && cargo build --release -p parcad-cli"
            .into()
    }
}

/// By extension, which is all a static bundle needs; the browser treats an
/// unknown type as a download, so the fallback is deliberately that.
fn mime_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("json") | Some("webmanifest") => "application/json",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

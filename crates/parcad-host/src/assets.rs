//! The frontend bundle, as whichever host embeds this crate carries it.

/// Where the frontend's bytes come from.
///
/// The desktop app answers from Tauri's asset resolver, `parcad serve` from a
/// copy of `app/dist` embedded when the CLI was built, and a browser tab from
/// the files its page hands the host (`page.rs`). The router does not know which, and that is what keeps the frontend one build: neither host can
/// serve a different bundle from the other's.
pub trait Assets: Send + Sync + 'static {
    fn get(&self, path: &str) -> Option<Asset>;
    /// What to do when `get` has nothing. The fix differs by host — a dev
    /// server to open, or a binary to rebuild — so the host says it.
    fn how_to_embed(&self) -> String;
}

pub struct Asset {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

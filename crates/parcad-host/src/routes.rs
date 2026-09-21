//! What the window's project and session requests do, whichever host serves them.
//!
//! `http.rs` answers these on a socket and `page.rs` inside a browser tab, so the
//! picker gets the same listing, and a save writes the same files, from either.

use crate::{projects, service, session};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct SaveRequest {
    pub script: String,
    #[serde(default)]
    pub readme: Option<String>,
    /// A `data:image/png;base64,` URL from the viewport canvas.
    #[serde(default)]
    pub preview: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum ProjectOp {
    Create { script: String },
    Folder,
    Rename { to: String },
    Title { title: String },
    Convert,
    Open,
}

#[derive(Deserialize)]
pub struct SessionPush {
    pub name: Option<String>,
    pub script: String,
    /// The pushing viewer's own id, echoed in the broadcast so that viewer can
    /// ignore its reflection.
    pub origin: String,
    /// The revision the script was edited from; see `session::push`.
    #[serde(default)]
    pub base: Option<u64>,
}

pub fn list_projects() -> Result<Value, String> {
    Ok(json!({
        // The flat list is what MCP answers with and what a caller that only
        // wants names can use; the tree is the same parts with their folders.
        "projects": projects::list()?,
        "tree": projects::tree()?,
        "directory": projects::dir().to_string_lossy(),
    }))
}

/// The editor a part's source would open in, for the titlebar to show. See
/// `service::editor`.
pub fn editor() -> Value {
    json!(service::editor())
}

pub fn read_project(name: &str) -> Result<Value, String> {
    let script = projects::read(name)?;
    Ok(json!({ "name": name, "script": script }))
}

pub fn save_project(name: &str, request: SaveRequest) -> Result<Value, String> {
    let path = projects::write(name, &request.script)?;
    if let Some(readme) = request.readme {
        projects::write_readme(name, &readme)?;
    }
    if let Some(preview) = request.preview {
        projects::write_preview_data_url(name, &preview)?;
    }
    Ok(json!({ "name": name, "path": path }))
}

pub fn project_op(name: &str, request: ProjectOp) -> Result<Value, String> {
    let (renamed, path) = match request {
        ProjectOp::Create { script } => (name.to_string(), projects::create(name, &script)),
        ProjectOp::Folder => (name.to_string(), projects::create_folder(name)),
        ProjectOp::Rename { to } => (to.clone(), projects::rename(name, &to)),
        ProjectOp::Title { title } => (
            name.to_string(),
            projects::set_title(name, &title).map(|()| name.to_string()),
        ),
        ProjectOp::Convert => (name.to_string(), projects::convert(name)),
        // The one op that answers with something besides a path: which editor
        // took the file, since it is a choice the user did not make here.
        ProjectOp::Open => return open_source(name),
    };
    Ok(json!({ "name": renamed, "path": path? }))
}

/// Hand the part's source to the user's own editor. See `service::open_in_editor`.
fn open_source(name: &str) -> Result<Value, String> {
    let path = projects::source_path(name)?.to_string_lossy().to_string();
    let opened_with = service::open_in_editor(&path)?;
    Ok(json!({ "name": name, "path": path, "opened_with": opened_with }))
}

pub fn delete_project(name: &str) -> Result<Value, String> {
    let trashed = projects::remove(name)?;
    Ok(json!({ "name": name, "trashed": trashed }))
}

/// The thumbnail on its own, without touching the script.
///
/// The app writes one the first time it draws a part that has none, so a folder
/// of parts nobody has edited yet still shows what they are. Rewriting
/// `part.js` to do that would touch the user's source — and its mtime, which
/// the picker reports — for a picture.
pub fn save_project_preview(name: &str, preview: &str) -> Result<Value, String> {
    projects::write_preview_data_url(name, preview)?;
    Ok(json!({ "name": name }))
}

pub fn push_session(request: SessionPush) -> session::Session {
    session::push(request.name, request.script, request.origin, request.base)
}

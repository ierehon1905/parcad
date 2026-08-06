//! Desktop backend. Thin by design: it owns no modelling logic, it only moves
//! an intent graph into a geometry backend and geometry back out to a viewport.
//!
//! There are two viewports. The Tauri webview reaches `service` over IPC; a
//! browser reaches the same functions over the local port `http` hosts. The
//! commands below are therefore adapters and nothing else — any behaviour that
//! lived here would be a feature the desktop had and the browser did not.

mod http;
mod mcp;
mod projects;
mod script;
mod service;
mod session;

use service::{Backend, Evaluated};

#[tauri::command]
fn evaluate(
    graph: serde_json::Value,
    depth: u8,
    backend: Option<String>,
) -> Result<Evaluated, String> {
    let backend = Backend::parse(backend.as_deref())?;
    service::evaluate(&service::parse_graph(graph)?, depth, backend)
}

#[tauri::command]
fn inspect_edge_target(
    graph: serde_json::Value,
    node: usize,
) -> Result<parcad_occt::TargetPreview, String> {
    service::inspect_edge_target(&service::parse_graph(graph)?, node)
}

/// Write the current part out as a binary STL, where the desktop asked for it.
#[tauri::command]
fn export_stl(
    graph: serde_json::Value,
    depth: u8,
    path: String,
    backend: Option<String>,
) -> Result<String, String> {
    let backend = Backend::parse(backend.as_deref())?;
    let export = service::export_stl(&service::parse_graph(graph)?, depth, backend)?;
    service::write_export(&export, &path)
}

/// Write the current part out as STEP.
#[tauri::command]
fn export_step(graph: serde_json::Value, path: String) -> Result<String, String> {
    let export = service::export_step(&service::parse_graph(graph)?)?;
    service::write_export(&export, &path)
}

/// The project folder, shared with the browser host and with MCP.
///
/// The webview cannot reach `/api` — its origin is `tauri://localhost`, not the
/// HTTP host — so projects need an IPC adapter like everything else. Both call
/// the same `projects` functions and read the same directory.
#[tauri::command]
fn list_projects() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "projects": projects::list()?,
        "tree": projects::tree()?,
        "directory": projects::dir().to_string_lossy(),
    }))
}

#[tauri::command]
fn read_project(name: String) -> Result<serde_json::Value, String> {
    let script = projects::read(&name)?;
    Ok(serde_json::json!({ "name": name, "script": script }))
}

/// Save a part, and the two derived files that go beside it.
///
/// One call rather than three: a bundle whose `README.md` describes a shape its
/// `part.js` no longer builds is worse than one with no README at all, and the
/// only way to keep them together is to write them together. Both are optional
/// because a loose `.js` has nowhere to put either.
#[tauri::command]
fn save_project(
    name: String,
    script: String,
    readme: Option<String>,
    preview: Option<String>,
) -> Result<serde_json::Value, String> {
    let path = projects::write(&name, &script)?;
    if let Some(readme) = readme {
        projects::write_readme(&name, &readme)?;
    }
    if let Some(preview) = preview {
        projects::write_preview_data_url(&name, &preview)?;
    }
    Ok(serde_json::json!({ "name": name, "path": path }))
}

#[tauri::command]
fn create_project(name: String, script: String) -> Result<serde_json::Value, String> {
    let path = projects::create(&name, &script)?;
    Ok(serde_json::json!({ "name": name, "path": path }))
}

#[tauri::command]
fn create_folder(name: String) -> Result<serde_json::Value, String> {
    let path = projects::create_folder(&name)?;
    Ok(serde_json::json!({ "name": name, "path": path }))
}

#[tauri::command]
fn rename_project(name: String, to: String) -> Result<serde_json::Value, String> {
    let path = projects::rename(&name, &to)?;
    Ok(serde_json::json!({ "name": to, "path": path }))
}

#[tauri::command]
fn delete_project(name: String) -> Result<serde_json::Value, String> {
    let path = projects::remove(&name)?;
    Ok(serde_json::json!({ "name": name, "trashed": path }))
}

#[tauri::command]
fn set_project_title(name: String, title: String) -> Result<(), String> {
    projects::set_title(&name, &title)
}

#[tauri::command]
fn convert_project(name: String) -> Result<serde_json::Value, String> {
    let path = projects::convert(&name)?;
    Ok(serde_json::json!({ "name": name, "path": path }))
}

/// The thumbnail alone. See the HTTP adapter's `save_project_preview` for why
/// this does not go through `save_project`.
#[tauri::command]
fn save_project_preview(name: String, preview: String) -> Result<(), String> {
    projects::write_preview_data_url(&name, &preview)
}

/// A part's thumbnail as a data URL, asked for one card at a time.
///
/// Not folded into the listing: nineteen base64 PNGs would make opening the
/// picker a megabyte of JSON to show a dozen cards that fit on screen.
#[tauri::command]
fn project_preview(name: String) -> Result<String, String> {
    use base64::Engine;
    let png = projects::preview(&name)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

/// Whether a model is connected over MCP, for the window to show.
///
/// The webview's origin is `tauri://localhost`, so it cannot read `/api/mcp`
/// the way a browser does — same function, second adapter.
#[tauri::command]
fn mcp_status() -> mcp::Status {
    service::mcp_status()
}

/// The live session, over IPC. The webview cannot open an EventSource against
/// `/api` — its origin is `tauri://localhost` — so it pushes here and receives
/// broadcasts as the Tauri event the setup hook below forwards.
#[tauri::command]
fn get_session() -> session::Session {
    session::get()
}

#[tauri::command]
fn push_session(name: Option<String>, script: String, origin: String) -> session::Session {
    session::push(name, script, origin)
}

/// Where the frontend should send API calls, injected before it loads.
///
/// Under IPC the answer is "nowhere, use invoke"; the browser learns its own
/// origin from the page it was served. This exists so the editor never has to
/// guess a port number.
#[tauri::command]
fn host_port() -> u16 {
    http::port()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // WebDriver is an end-to-end test transport, not an application feature.
    // Keep its HTTP server out of release binaries even if somebody happens to
    // pass `--features e2e` to a release build.
    #[cfg(all(feature = "e2e", debug_assertions))]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .setup(|app| {
            // Seed before the host comes up: the frontend asks for the project
            // list as it loads, and an empty first launch would look like a
            // fresh install with nothing in it.
            if let Err(e) = projects::seed() {
                eprintln!("parcad: could not prepare the project folder: {e}");
            }
            http::serve(app.handle().clone());
            // One broadcast, two transports: browsers get SSE from the HTTP
            // host, the webview gets this Tauri event. Forwarded here because
            // emitting *is* the IPC transport's delivery — the capability
            // (state, revision, echo rule) stays in `session`.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                let mut events = session::subscribe();
                loop {
                    match events.recv().await {
                        Ok(event) => {
                            let _ = handle.emit("session-changed", &event);
                        }
                        // A lagged webview missed intermediate states, not the
                        // final one: the next event carries the whole session.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            evaluate,
            inspect_edge_target,
            export_stl,
            export_step,
            list_projects,
            read_project,
            save_project,
            create_project,
            create_folder,
            rename_project,
            delete_project,
            set_project_title,
            convert_project,
            project_preview,
            save_project_preview,
            mcp_status,
            get_session,
            push_session,
            host_port
        ])
        .run(tauri::generate_context!())
        .expect("error while running parcad");
}

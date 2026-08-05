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
        "directory": projects::dir().to_string_lossy(),
    }))
}

#[tauri::command]
fn read_project(name: String) -> Result<serde_json::Value, String> {
    let script = projects::read(&name)?;
    Ok(serde_json::json!({ "name": name, "script": script }))
}

#[tauri::command]
fn save_project(name: String, script: String) -> Result<serde_json::Value, String> {
    let path = projects::write(&name, &script)?;
    Ok(serde_json::json!({ "name": name, "path": path }))
}

/// Whether a model is connected over MCP, for the window to show.
///
/// The webview's origin is `tauri://localhost`, so it cannot read `/api/mcp`
/// the way a browser does — same function, second adapter.
#[tauri::command]
fn mcp_status() -> mcp::Status {
    service::mcp_status()
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
            mcp_status,
            host_port
        ])
        .run(tauri::generate_context!())
        .expect("error while running parcad");
}

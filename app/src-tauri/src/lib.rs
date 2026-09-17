//! Desktop backend. Thin by design: it owns no modelling logic, it only moves
//! an intent graph into a geometry backend and geometry back out to a viewport.
//!
//! There are two viewports. The Tauri webview reaches `service` over IPC; a
//! browser reaches the same functions over the local port `http` hosts. The
//! commands below are therefore adapters and nothing else — any behaviour that
//! lived here would be a feature the desktop had and the browser did not. The
//! capabilities and both hosts are `parcad-host`, which `parcad serve` embeds
//! too, with no window.

use parcad_host::{http, mcp, projects, service, session};
use service::Evaluated;

#[tauri::command]
fn evaluate(graph: serde_json::Value) -> Result<Evaluated, String> {
    service::evaluate(&service::parse_graph(graph)?, None)
}

#[tauri::command]
fn inspect_edge_target(
    graph: serde_json::Value,
    node: usize,
) -> Result<parcad_occt::TargetPreview, String> {
    service::inspect_edge_target(&service::parse_graph(graph)?, node)
}

/// Write the current part out as a binary STL, beside the part it came from.
#[tauri::command]
fn export_stl(graph: serde_json::Value, project: String) -> Result<String, String> {
    let export = service::export_stl(&service::parse_graph(graph)?, None)?;
    write_and_reveal(&export, &project, "stl")
}

/// Write the current part out as 3MF, one object per body, beside the part.
#[tauri::command]
fn export_3mf(graph: serde_json::Value, project: String) -> Result<String, String> {
    let name = project.rsplit('/').next().unwrap_or(&project);
    let export = service::export_3mf(&service::parse_graph(graph)?, None, name)?;
    write_and_reveal(&export, &project, "3mf")
}

/// Write the current part out as STEP.
#[tauri::command]
fn export_step(graph: serde_json::Value, project: String) -> Result<String, String> {
    let export = service::export_step(&service::parse_graph(graph)?)?;
    write_and_reveal(&export, &project, "step")
}

/// Put an export where the part is, and show it to the user.
///
/// The path is resolved from the project rather than passed in from the
/// webview: the frontend knows which part is open, not where the project folder
/// lives, and the one previous caller passed a bare `"part.stl"` that a bundled
/// app resolved against whatever working directory macOS had given it.
///
/// Revealing is best-effort on purpose. The bytes are on disk and the absolute
/// path is what this returns, so a machine with no file manager reports the
/// export as done — which it is — instead of reporting a failure of the wrong
/// thing.
fn write_and_reveal(
    export: &service::Export,
    project: &str,
    extension: &str,
) -> Result<String, String> {
    let path = projects::export_path(project, extension)?;
    let written = service::write_export(export, &path.to_string_lossy())?;
    let _ = service::reveal(&written);
    Ok(written)
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
fn get_session() -> session::Live {
    session::live()
}

#[tauri::command]
fn push_session(
    name: Option<String>,
    script: String,
    origin: String,
    base: Option<u64>,
) -> session::Session {
    session::push(name, script, origin, base)
}

#[tauri::command]
fn report_shown(shown: session::Shown) {
    session::report_shown(shown);
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

/// A release newer than this app, as the updater last found it.
///
/// Kept so installing downloads what the prompt announced, not whatever the
/// endpoint says a minute later.
#[derive(Default)]
struct PendingUpdate(std::sync::Mutex<Option<tauri_plugin_updater::Update>>);

/// Ask the release endpoint whether there is a newer app. `null` when there is
/// none, and always in a debug build, which is not a bundle an update can replace.
#[tauri::command]
async fn check_for_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    pending: tauri::State<'_, PendingUpdate>,
) -> Result<Option<serde_json::Value>, String> {
    use tauri_plugin_updater::UpdaterExt;
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let update = app
        .updater()
        .map_err(|e| format!("the updater is not configured: {e}"))?
        .check()
        .await
        .map_err(|e| format!("could not check for an update: {e}"))?;
    let reply = update.as_ref().map(|u| {
        serde_json::json!({ "version": u.version, "current": u.current_version, "notes": u.body })
    });
    *pending.0.lock().unwrap() = update;
    Ok(reply)
}

/// Download, verify and install the update `check_for_update` found, then
/// relaunch. On Windows the installer exits this process itself and restarts it.
#[tauri::command]
async fn install_update<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    pending: tauri::State<'_, PendingUpdate>,
) -> Result<(), String> {
    let update = pending.0.lock().unwrap().clone().ok_or(
        "there is no update to install: check for one first, and if this repeats, \
         download the latest release from https://github.com/ierehon1905/parcad/releases",
    )?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| {
            format!(
                "the update to {} did not install: {e}\n\
                 This copy of ParCAD is unchanged. Download the release by hand from \
                 https://github.com/ierehon1905/parcad/releases/tag/v{}",
                update.version, update.version
            )
        })?;
    app.restart()
}

/// Host the UI, API and MCP, waiting briefly for a port the previous process
/// still holds: an updated app is launched before the old one has exited.
async fn serve_http<R: tauri::Runtime>(assets: std::sync::Arc<TauriAssets<R>>) {
    let mut attempts = 0;
    loop {
        match http::serve(http::port(), assets.clone()).await {
            Err(e) if attempts < 10 && e.contains("could not host") => {
                attempts += 1;
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
            Err(e) => {
                eprintln!("parcad: {e}\nThe desktop window still works.");
                return;
            }
            Ok(()) => return,
        }
    }
}

/// Say so when this window is pointed at a dev server that is not running.
///
/// Tauri's dev/production switch is the `custom-protocol` feature the tauri CLI
/// adds to the `cargo build` it runs — *not* the cargo profile. So a plain
/// `cargo build --release -p parcad-app` yields a binary that embeds no
/// frontend and whose window loads `devUrl`. Launched without Vite on that
/// port, the window comes up, paints nothing, and says nothing, while the HTTP
/// host below still serves the UI by reading `app/dist` off disk. "Browser
/// works, webview dead" reads as a broken webview and is nothing of the kind;
/// the fix is a build flag, so name it. See docs/GOTCHAS.md.
fn warn_if_the_window_awaits_a_dev_server<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;

    // A production window is on `tauri://localhost`. Only a dev one is on http,
    // so anything else here is already the case we do not need to warn about.
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(url) = window.url() else { return };
    if !matches!(url.scheme(), "http" | "https") {
        return;
    }

    let default_port = if url.scheme() == "https" { 443 } else { 80 };
    let Ok(addrs) = url.socket_addrs(|| Some(default_port)) else {
        return;
    };
    // Loopback refuses immediately, which is the case this exists for; the
    // timeout only bounds a host that does not answer at all.
    if addrs.iter().any(|addr| {
        std::net::TcpStream::connect_timeout(addr, std::time::Duration::from_millis(300)).is_ok()
    }) {
        return;
    }

    eprintln!(
        "parcad: the desktop window will stay blank, and this is a build flag, not a bug.\n\
         \n\
         This binary was built without tauri's `custom-protocol` feature — the one the\n\
         tauri CLI adds, and the one that embeds the frontend. Its window therefore\n\
         loads {url}, where nothing is listening.\n\
         The HTTP host above is unaffected: a browser on it is the whole\n\
         application, not a reduced one.\n\
         \n\
         Embed the frontend, so the binary carries its own UI:\n\
         \x20 cd app && bun run tauri build\n\
         \x20 cargo build --release -p parcad-app --features tauri/custom-protocol\n\
         or start the dev server this window is waiting for:\n\
         \x20 cd app && bun run tauri dev"
    );
}

/// The frontend Tauri already carries, offered to the HTTP host.
///
/// Resolving through Tauri's asset resolver rather than a static directory keeps
/// exactly one copy of the frontend in the app: the browser is served the same
/// bytes the webview loads.
struct TauriAssets<R: tauri::Runtime>(tauri::AppHandle<R>);

impl<R: tauri::Runtime> http::Assets for TauriAssets<R> {
    fn get(&self, path: &str) -> Option<http::Asset> {
        let asset = self.0.asset_resolver().get(path.to_string())?;
        Some(http::Asset {
            mime_type: asset.mime_type,
            bytes: asset.bytes,
        })
    }

    fn how_to_embed(&self) -> String {
        "Under `tauri dev` the UI is served by Vite: open http://localhost:1420 instead — \
         it proxies /api to this port.\n\
         To serve the UI from here, build the frontend first: cd app && bun run build"
            .into()
    }
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(PendingUpdate::default())
        .setup(|app| {
            // Seed before the host comes up: the frontend asks for the project
            // list as it loads, and an empty first launch would look like a
            // fresh install with nothing in it.
            if let Err(e) = projects::seed() {
                eprintln!("parcad: could not prepare the project folder: {e}");
            }
            // A failure to bind is not fatal — the window works regardless —
            // but it must be visible, because the symptom otherwise is a
            // browser tab that cannot connect and nothing explaining why.
            let assets = std::sync::Arc::new(TauriAssets(app.handle().clone()));
            tauri::async_runtime::spawn(serve_http(assets));
            // After the host, so the message can point at it as the way out.
            warn_if_the_window_awaits_a_dev_server(app.handle());
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
            export_3mf,
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
            report_shown,
            host_port,
            check_for_update,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running parcad");
}

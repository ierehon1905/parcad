//! The same application, reachable from a browser.
//!
//! The desktop process hosts its own UI and API on a local port. A browser
//! pointed at that port loads the identical frontend bundle and calls the
//! identical `service` functions the webview calls over IPC — so "the browser
//! version" is not a second implementation with fewer features, it is a second
//! window onto this one.
//!
//! Three deliberate constraints:
//!
//! - **Loopback only.** This endpoint evaluates arbitrary intent graphs, which
//!   means spawning the OCCT worker. That is a local tool, not a service; it is
//!   never bound to a routable address.
//! - **No CORS headers.** Their absence is the access control. A page on another
//!   origin can still *send* a JSON POST, but the preflight it requires will
//!   fail and it can never read a reply. Adding a permissive layer here would
//!   hand every open tab a geometry kernel.
//! - **No caller-supplied paths.** The IPC transport writes an export where the
//!   desktop user pointed; HTTP returns the bytes and lets the browser save
//!   them. A path parameter on a socket is an arbitrary-write primitive.

use crate::mcp;
use crate::projects;
use crate::service::{self, Backend};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Runtime};

/// The port the UI and API are hosted on, overridable for a second instance.
pub fn port() -> u16 {
    std::env::var("PARCAD_HTTP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4242)
}

#[derive(Deserialize)]
struct EvaluateRequest {
    graph: serde_json::Value,
    #[serde(default = "default_depth")]
    depth: u8,
    backend: Option<String>,
}

fn default_depth() -> u8 {
    7
}

#[derive(Deserialize)]
struct InspectRequest {
    graph: serde_json::Value,
    node: usize,
}

#[derive(Deserialize)]
struct SaveRequest {
    script: String,
}

#[derive(Deserialize)]
struct StepRequest {
    graph: serde_json::Value,
}

/// Start the host, and say plainly on stderr whether it came up.
///
/// A failure to bind is not fatal — the desktop window works regardless — but it
/// must be visible, because the symptom otherwise is a browser tab that cannot
/// connect and nothing anywhere explaining why. The usual cause is a second
/// instance already holding the port, which the message names.
pub fn serve<R: Runtime>(app: AppHandle<R>) {
    let port = port();
    tauri::async_runtime::spawn(async move {
        let router = router(app);
        let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => {
                eprintln!("parcad: UI and API hosted on http://127.0.0.1:{port}");
                if let Err(e) = axum::serve(listener, router).await {
                    eprintln!("parcad: the HTTP host stopped: {e}");
                }
            }
            Err(e) => eprintln!(
                "parcad: could not host on 127.0.0.1:{port}: {e}\n\
                 The desktop window still works. If another parcad is already running, \
                 use that one, or start this instance with PARCAD_HTTP_PORT=<other port>."
            ),
        }
    });
}

fn router<R: Runtime>(app: AppHandle<R>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        // Whether a model is connected to the MCP endpoint below. Read by both
        // windows; the desktop one gets it over IPC instead.
        .route("/api/mcp", get(mcp_status))
        .route("/api/evaluate", post(evaluate))
        .route("/api/inspect-edge-target", post(inspect_edge_target))
        .route("/api/export/stl", post(export_stl))
        .route("/api/export/step", post(export_step))
        // Projects are files on disk shared with the desktop window and with
        // MCP. The frontend reads them from here rather than from a build-time
        // glob, so a part an agent saves shows up in the picker.
        .route("/api/projects", get(list_projects))
        .route("/api/projects/{name}", get(read_project).put(save_project))
        // The same application again, for a model rather than a person. It
        // reaches `service` through the same functions, and the scripts it
        // sends run in `script`'s sandbox rather than the webview.
        .nest_service("/mcp", mcp::service())
        // Everything else is the frontend. Registered last and as a fallback so
        // no asset name can ever shadow an API route.
        .fallback(asset)
        .with_state(app)
}

async fn health() -> impl IntoResponse {
    Json(json!({
        "app": "parcad",
        "version": env!("CARGO_PKG_VERSION"),
        // The frontend reads this to confirm it is talking to a real host
        // before it decides it has a backend at all.
        "transport": "http",
    }))
}

async fn mcp_status() -> impl IntoResponse {
    Json(service::mcp_status())
}

/// Geometry work is blocking and slow — the OCCT path is a whole subprocess.
///
/// Running it on the async runtime's threads would let one fillet stall every
/// other request, including the asset requests that load the page.
async fn blocking<T, F>(work: F) -> Result<T, Failed>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result.map_err(Failed),
        Err(e) => Err(Failed(format!("the evaluation task did not finish: {e}"))),
    }
}

async fn evaluate(Json(request): Json<EvaluateRequest>) -> Result<Response, Failed> {
    let backend = Backend::parse(request.backend.as_deref()).map_err(Failed)?;
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let evaluated = blocking(move || service::evaluate(&doc, request.depth, backend)).await?;
    Ok(Json(evaluated).into_response())
}

async fn inspect_edge_target(Json(request): Json<InspectRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let preview = blocking(move || service::inspect_edge_target(&doc, request.node)).await?;
    Ok(Json(preview).into_response())
}

async fn export_stl(Json(request): Json<EvaluateRequest>) -> Result<Response, Failed> {
    let backend = Backend::parse(request.backend.as_deref()).map_err(Failed)?;
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let export = blocking(move || service::export_stl(&doc, request.depth, backend)).await?;
    Ok(download(export))
}

async fn export_step(Json(request): Json<StepRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let export = blocking(move || service::export_step(&doc)).await?;
    Ok(download(export))
}

fn download(export: service::Export) -> Response {
    (
        [
            (header::CONTENT_TYPE, export.content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", export.filename),
            ),
        ],
        export.bytes,
    )
        .into_response()
}

async fn list_projects() -> Result<Response, Failed> {
    let projects = projects::list().map_err(Failed)?;
    Ok(Json(json!({
        "projects": projects,
        "directory": projects::dir().to_string_lossy(),
    }))
    .into_response())
}

async fn read_project(Path(name): Path<String>) -> Result<Response, Failed> {
    let script = projects::read(&name).map_err(Failed)?;
    Ok(Json(json!({ "name": name, "script": script })).into_response())
}

async fn save_project(
    Path(name): Path<String>,
    Json(request): Json<SaveRequest>,
) -> Result<Response, Failed> {
    let path = projects::write(&name, &request.script).map_err(Failed)?;
    Ok(Json(json!({ "name": name, "path": path })).into_response())
}

/// Serve the frontend bundle Tauri already carries.
///
/// Resolving through Tauri's asset resolver rather than a static directory keeps
/// exactly one copy of the frontend: the browser is served the same bytes the
/// webview loads, so the two hosts cannot drift to different builds.
async fn asset<R: Runtime>(State(app): State<AppHandle<R>>, uri: Uri) -> Response {
    let path = match uri.path() {
        "/" => "index.html".to_string(),
        other => other.trim_start_matches('/').to_string(),
    };

    // A client-side route or a reloaded deep link is not a missing file.
    let resolved = app
        .asset_resolver()
        .get(path)
        .or_else(|| app.asset_resolver().get("index.html".into()));

    match resolved {
        Some(asset) => ([(header::CONTENT_TYPE, asset.mime_type)], asset.bytes).into_response(),
        // In `tauri dev` the frontend is served by Vite and only bundled at
        // build time, so there may be nothing to resolve. Say where it is
        // instead of returning a bare 404.
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!(
                "parcad is running, but no frontend bundle is embedded in this binary.\n\
                 Under `tauri dev` the UI is served by Vite: open http://localhost:1420 \
                 instead — it proxies /api to this port.\n\
                 To serve the UI from here, build the frontend first: cd app && bun run build\n",
            ),
        )
            .into_response(),
    }
}

/// An error the caller should read verbatim.
///
/// The service layer's messages name the fix, so they are passed through whole
/// rather than being reduced to a status code. 422 rather than 400: the request
/// was well-formed JSON, the geometry in it was refused.
struct Failed(String);

impl IntoResponse for Failed {
    fn into_response(self) -> Response {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": self.0 })),
        )
            .into_response()
    }
}

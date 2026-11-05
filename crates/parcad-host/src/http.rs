//! The same application, reachable from a browser.
//!
//! The process hosts its own UI and API on a local port. A browser pointed at
//! that port loads the identical frontend bundle and calls the identical
//! `service` functions the webview calls over IPC — so "the browser version"
//! is not a second implementation with fewer features, it is a second window
//! onto this one. Two processes host it: the desktop app, whose Tauri runtime
//! already carries the frontend, and `parcad serve`, which carries a copy of
//! the same build and no window.
//!
//! Three deliberate constraints:
//!
//! - **Loopback, unless every request is somebody's.** This endpoint evaluates
//!   arbitrary intent graphs, which means spawning the OCCT worker, so a local
//!   tool never binds a routable address. The one exception is hosted mode:
//!   `parcad serve --hosted` binds where it is told, and then `/mcp` and every
//!   `/api` route sit behind [`authenticate`], which resolves a bearer token to
//!   one account's [`tenant::Tenant`] or answers 401 naming the fix. Only
//!   `/api/health` stays open, and no frontend is served at all — a browser
//!   cannot send a bearer. What an account wrote it fetches back from
//!   `/api/exports/<name>` with the same bearer, which is the one way a file on
//!   the server reaches a client.
//! - **No CORS headers.** Their absence is the access control. A page on another
//!   origin can still *send* a JSON POST, but the preflight it requires will
//!   fail and it can never read a reply. Adding a permissive layer here would
//!   hand every open tab a geometry kernel.
//! - **No caller-supplied paths.** The IPC transport writes an export where the
//!   desktop user pointed; HTTP returns the bytes and lets the browser save
//!   them. A path parameter on a socket is an arbitrary-write primitive.

use crate::mcp;
use crate::projects;
use crate::routes::{self, ProjectOp, SaveRequest, SessionPush};
use crate::service;
use crate::session;
use crate::tenant;
use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

pub use crate::assets::{Asset, Assets};

/// The port the UI and API are hosted on, overridable for a second instance.
pub fn port() -> u16 {
    std::env::var("PARCAD_HTTP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4242)
}

/// Where a local host listens: this machine only.
pub fn loopback(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[derive(Deserialize)]
struct EvaluateRequest {
    graph: serde_json::Value,
}

#[derive(Deserialize)]
struct InspectRequest {
    graph: serde_json::Value,
    node: usize,
}

#[derive(Deserialize)]
struct StepRequest {
    graph: serde_json::Value,
}

/// Host the application on `address` until the listener stops, saying on
/// stderr where it is. `accounts` makes it a server for many accounts; the one
/// caller that binds anything but loopback, `parcad serve`, refuses to without them.
///
/// A failure to bind comes back as the error rather than a log line, because
/// the two callers disagree about how bad it is: the desktop window works
/// regardless, while `parcad serve` has nothing else to do. The usual cause is
/// a second instance already holding the port, which the message names.
pub async fn serve(address: SocketAddr, assets: Arc<dyn Assets>, accounts: Option<tenant::Accounts>) -> Result<(), String> {
    let announcement = match &accounts {
        None => format!("parcad: UI and API hosted on http://{address}, MCP at /mcp"),
        Some(accounts) => format!(
            "parcad: hosted on http://{address} as https://{}/mcp for {} tokens; /mcp and /api need a bearer",
            accounts.public_host(),
            accounts.tokens()
        ),
    };
    let router = router(assets, accounts);
    let listener = tokio::net::TcpListener::bind(address).await.map_err(|e| {
        format!(
            "could not host on {address}: {e}\n\
             If another parcad is already running, use that one, or start this \
             instance with PARCAD_HTTP_PORT=<other port>."
        )
    })?;
    eprintln!("{announcement}");
    axum::serve(listener, router)
        .await
        .map_err(|e| format!("the HTTP host stopped: {e}"))
}

fn router(assets: Arc<dyn Assets>, accounts: Option<tenant::Accounts>) -> Router {
    let Some(accounts) = accounts else {
        return Router::new()
            .route("/api/health", get(health))
            .merge(api())
            // The same application again, for a model rather than a person. It
            // reaches `service` through the same functions, and the scripts it
            // sends run in `script`'s sandbox rather than the webview.
            .nest_service("/mcp", mcp::service(assets.clone(), None))
            // Everything else is the frontend. Registered last and as a fallback so
            // no asset name can ever shadow an API route.
            .fallback(asset)
            .with_state(assets);
    };
    // Every `/api` route too, not `/mcp` alone: an agent service builds and
    // exports a part over them. No frontend, which could carry no bearer.
    let guarded = api()
        .nest_service("/mcp", mcp::service(assets, Some(accounts.public_host())))
        .layer(axum::middleware::from_fn_with_state(Arc::new(accounts), authenticate));
    Router::new().route("/api/health", get(health)).merge(guarded)
}

/// The routes behind the window and, hosted, behind a bearer. Generic in the
/// state because none of them reads any; the local router adds the frontend's.
fn api<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        // Whether a model is connected to the MCP endpoint. Read by both
        // windows; the desktop one gets it over IPC instead.
        .route("/api/mcp", get(mcp_status))
        // The live session: what is on screen, pushed by viewers on the
        // editor's debounce, and streamed back out as SSE so every browser tab
        // follows a change whichever caller made it. The webview gets the same
        // broadcast as a Tauri event instead — one broadcast, two transports.
        .route("/api/session", get(get_session).post(push_session))
        .route("/api/session/events", get(session_events))
        .route("/api/session/shown", post(session_shown))
        .route("/api/evaluate", post(evaluate))
        .route("/api/built/{*name}", get(build_project))
        .route("/api/inspect-edge-target", post(inspect_edge_target))
        .route("/api/export/stl", post(export_stl))
        .route("/api/export/3mf", post(export_3mf))
        .route("/api/export/step", post(export_step))
        // What export_part and a reply's `markdown` wrote, back as a download:
        // the only way a file in a hosted account's folder reaches its client.
        .route("/api/exports/{*name}", get(export_file))
        // Projects are files on disk shared with the desktop window and with
        // MCP. The frontend reads them from here rather than from a build-time
        // glob, so a part an agent saves shows up in the picker.
        .route("/api/projects", get(list_projects))
        .route("/api/editor", get(editor))
        // A project path may name folders, so it is a wildcard. Nothing can
        // follow one in a route, which is why the thumbnail has a prefix of its
        // own and everything else is a tagged POST.
        .route(
            "/api/projects/{*name}",
            get(read_project)
                .put(save_project)
                .post(project_op)
                .delete(delete_project),
        )
        .route(
            "/api/preview/{*name}",
            get(project_preview).put(save_project_preview),
        )
}

/// Resolve the request's bearer to its account, or answer for it. The tenant
/// is put on the request for rmcp, which serves each `/mcp` request on a task
/// of its own, and entered around `next` for the `/api` handlers, which run
/// right here.
async fn authenticate(
    State(accounts): State<Arc<tenant::Accounts>>,
    mut request: Request,
    next: Next,
) -> Response {
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .map(|value| value.to_str().unwrap_or(""));
    let tenant = match accounts.resolve(authorization) {
        Ok(tenant) => tenant,
        Err(refused) => return refused.into_response(),
    };
    request.extensions_mut().insert(tenant.clone());
    tenant::scoped(tenant, next.run(request)).await
}

impl IntoResponse for tenant::Refused {
    fn into_response(self) -> Response {
        let (status, challenge) = match &self {
            tenant::Refused::NoToken => (StatusCode::UNAUTHORIZED, Some("Bearer realm=\"parcad\"")),
            tenant::Refused::Unknown => (
                StatusCode::UNAUTHORIZED,
                Some("Bearer realm=\"parcad\", error=\"invalid_token\""),
            ),
            tenant::Refused::Unready(_) => (StatusCode::INTERNAL_SERVER_ERROR, None),
        };
        let mut response = (status, self.message()).into_response();
        if let Some(challenge) = challenge {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, header::HeaderValue::from_static(challenge));
        }
        response
    }
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

async fn get_session() -> impl IntoResponse {
    Json(session::live())
}

async fn push_session(Json(request): Json<SessionPush>) -> impl IntoResponse {
    Json(routes::push_session(request))
}

async fn session_shown(Json(shown): Json<session::Shown>) -> impl IntoResponse {
    session::report_shown(shown);
    Json(json!({}))
}

/// Forgets a tab's report when its event stream is dropped.
struct Departure(Option<String>);

impl Drop for Departure {
    fn drop(&mut self) {
        if let Some(id) = &self.0 {
            session::forget_viewer(id);
        }
    }
}

/// Session changes as they happen, for a browser tab to watch.
///
/// SSE rather than a websocket: the traffic is one-way, `EventSource`
/// reconnects on its own, and the reply stays ordinary HTTP under the same
/// no-CORS rule as everything else here.
async fn session_events(uri: Uri) -> impl IntoResponse {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use tokio_stream::StreamExt;

    // `?viewer=<id>`: the tab's viewer id, so its report goes when it does.
    let viewer = uri.query().and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("viewer="))
            .map(str::to_string)
    });
    let departure = Departure(viewer);
    // The current state first: `EventSource` reconnects on its own, and a tab
    // that was away for an edit would otherwise wait for the next one to see it.
    let current = tokio_stream::once(session::get());
    let changes = tokio_stream::wrappers::BroadcastStream::new(session::subscribe())
        // A lagged tab missed intermediate states, never the final one — the
        // next event carries the whole session, so skipping is correct.
        .filter_map(|event| event.ok());
    let events = current.chain(changes).map(move |event| {
        let _ = &departure;
        Event::default().json_data(&event)
    });

    Sse::new(events).keep_alive(KeepAlive::default())
}

/// Geometry work is blocking and slow — the OCCT path is a whole subprocess.
///
/// Running it on the async runtime's threads would let one fillet stall every
/// other request, including the asset requests that load the page. The
/// request's tenant is a task-local, which no blocking thread sees until it
/// is re-entered there (`tenant.rs`).
async fn blocking<T, F>(work: F) -> Result<T, Failed>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let tenant = tenant::current();
    match tokio::task::spawn_blocking(move || tenant::scope(tenant, work)).await {
        Ok(result) => result.map_err(Failed),
        Err(e) => Err(Failed(format!("the evaluation task did not finish: {e}"))),
    }
}

async fn evaluate(Json(request): Json<EvaluateRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let evaluated = blocking(move || service::evaluate(&doc, None)).await?;
    Ok(Json(evaluated).into_response())
}

async fn build_project(Path(name): Path<String>) -> Result<Response, Failed> {
    let evaluated = blocking(move || routes::build_project(&name)).await?;
    Ok(Json(evaluated).into_response())
}

async fn inspect_edge_target(Json(request): Json<InspectRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let preview = blocking(move || service::inspect_edge_target(&doc, request.node)).await?;
    Ok(Json(preview).into_response())
}

async fn export_stl(Json(request): Json<EvaluateRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let export = blocking(move || service::export_stl(&doc, None)).await?;
    Ok(download(export.filename, export.content_type, export.bytes))
}

async fn export_3mf(Json(request): Json<EvaluateRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let export = blocking(move || service::export_3mf(&doc, None, "part")).await?;
    Ok(download(export.filename, export.content_type, export.bytes))
}

async fn export_step(Json(request): Json<StepRequest>) -> Result<Response, Failed> {
    let doc = service::parse_graph(request.graph).map_err(Failed)?;
    let export = blocking(move || service::export_step(&doc)).await?;
    Ok(download(export.filename, export.content_type, export.bytes))
}

async fn export_file(Path(name): Path<String>) -> Response {
    match service::export_file(&name) {
        Ok(file) => download(&file.filename, file.content_type, file.bytes),
        Err(refused) => refused.into_response(),
    }
}

impl IntoResponse for service::ExportRefused {
    fn into_response(self) -> Response {
        let status = match &self {
            service::ExportRefused::NotAName { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            service::ExportRefused::Missing { .. } => StatusCode::NOT_FOUND,
            service::ExportRefused::Unreadable { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let mut response = Failed(self.message()).into_response();
        *response.status_mut() = status;
        response
    }
}

fn download(filename: &str, content_type: &str, bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        bytes,
    )
        .into_response()
}

async fn list_projects() -> Result<Response, Failed> {
    Ok(Json(routes::list_projects().map_err(Failed)?).into_response())
}

async fn editor() -> Response {
    Json(routes::editor()).into_response()
}

async fn read_project(Path(name): Path<String>) -> Result<Response, Failed> {
    Ok(Json(routes::read_project(&name).map_err(Failed)?).into_response())
}

async fn save_project(
    Path(name): Path<String>,
    Json(request): Json<SaveRequest>,
) -> Result<Response, Failed> {
    Ok(Json(routes::save_project(&name, request).map_err(Failed)?).into_response())
}

async fn project_op(
    Path(name): Path<String>,
    Json(request): Json<ProjectOp>,
) -> Result<Response, Failed> {
    Ok(Json(routes::project_op(&name, request).map_err(Failed)?).into_response())
}

async fn delete_project(Path(name): Path<String>) -> Result<Response, Failed> {
    Ok(Json(routes::delete_project(&name).map_err(Failed)?).into_response())
}

async fn save_project_preview(
    Path(name): Path<String>,
    Json(request): Json<routes::PreviewRequest>,
) -> Result<Response, Failed> {
    Ok(Json(routes::save_project_preview(&name, &request).map_err(Failed)?).into_response())
}

/// A part's thumbnail, as the image itself rather than base64 in JSON — this
/// one has a browser on the other end and `<img src>` is the whole point.
async fn project_preview(Path(name): Path<String>) -> Result<Response, Failed> {
    let png = projects::preview(&name).map_err(Failed)?;
    Ok(([(header::CONTENT_TYPE, "image/png")], png).into_response())
}

/// Serve the frontend bundle the host carries.
///
/// Resolving through the host's own copy rather than a directory on disk keeps
/// exactly one frontend: the browser is served the same bytes the webview
/// loads, so the two cannot drift to different builds.
async fn asset(State(assets): State<Arc<dyn Assets>>, uri: Uri) -> Response {
    let path = match uri.path() {
        "/" => "index.html",
        other => other.trim_start_matches('/'),
    };

    // A client-side route or a reloaded deep link is not a missing file.
    let resolved = assets.get(path).or_else(|| assets.get("index.html"));

    match resolved {
        Some(asset) => ([(header::CONTENT_TYPE, asset.mime_type)], asset.bytes).into_response(),
        // There may be nothing to resolve — under `tauri dev` Vite serves the
        // UI, and a CLI built before the frontend carries none. Say what to do
        // instead of returning a bare 404.
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!(
                "parcad is running, but this process carries no frontend bundle.\n{}\n",
                assets.how_to_embed()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant::tests::PUBLIC_HOST;
    use crate::tenant::TenantId;
    use std::io::{Read, Write};

    struct NoAssets;

    impl Assets for NoAssets {
        fn get(&self, _: &str) -> Option<Asset> {
            None
        }
        fn how_to_embed(&self) -> String {
            String::new()
        }
    }

    /// A router on a spare loopback port, served on a thread of its own for
    /// as long as the test process runs.
    fn on_a_spare_port(accounts: Option<tenant::Accounts>) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a spare port");
        let port = listener.local_addr().expect("the port").port();
        listener.set_nonblocking(true).expect("non-blocking");
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime");
            runtime.block_on(async {
                let listener = tokio::net::TcpListener::from_std(listener).expect("the listener");
                axum::serve(listener, router(Arc::new(NoAssets), accounts))
                    .await
                    .expect("the host runs");
            });
        });
        port
    }

    /// Two accounts in one tokens file, their tokens, and the host serving them.
    fn two_accounts() -> (std::path::PathBuf, String, String, u16) {
        let dir = crate::tenant::tests::fresh_root("hosted");
        let file = dir.join("tokens");
        let acme = tenant::issue(TenantId::new("acme").unwrap(), &file).unwrap();
        let bolt = tenant::issue(TenantId::new("bolt").unwrap(), &file).unwrap();
        let accounts = tenant::Accounts::read(&file, &dir.join("tenants"), PUBLIC_HOST).unwrap();
        let port = on_a_spare_port(Some(accounts));
        (dir, acme, bolt, port)
    }

    struct Reply {
        status: u16,
        head: String,
        bytes: Vec<u8>,
        body: String,
    }

    impl Reply {
        fn header(&self, name: &str) -> Option<&str> {
            self.head.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case(name).then(|| value.trim())
            })
        }
    }

    /// One HTTP/1.1 request over a socket of its own, read whole.
    fn exchange(port: u16, verb: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Reply {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("the host listens");
        let mut request = format!("{verb} {path} HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
        if !headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("host")) {
            request.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
        }
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream.write_all(request.as_bytes()).expect("the request is sent");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).expect("the reply is read");
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").expect("a response head");
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        let body = &raw[split + 4..];
        let status = head.split_whitespace().nth(1).and_then(|code| code.parse().ok()).expect("a status");
        let chunked = head.to_ascii_lowercase().contains("transfer-encoding: chunked");
        let bytes = if chunked { dechunk(body) } else { body.to_vec() };
        Reply {
            status,
            head,
            body: String::from_utf8_lossy(&bytes).into_owned(),
            bytes,
        }
    }

    fn dechunk(mut body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let line_end = body.windows(2).position(|w| w == b"\r\n").expect("a chunk size");
            let size = std::str::from_utf8(&body[..line_end]).expect("a chunk size in ASCII");
            let size = usize::from_str_radix(size.trim(), 16).expect("a hex chunk size");
            let rest = &body[line_end + 2..];
            if size == 0 {
                return out;
            }
            out.extend_from_slice(&rest[..size]);
            body = &rest[size + 2..];
        }
    }

    fn bearer(token: &str) -> String {
        format!("Bearer {token}")
    }

    /// Everything but `/api/health` is behind a bearer, and each refusal
    /// says what to do. No frontend is served.
    #[test]
    fn a_hosted_host_refuses_a_request_without_its_account_naming_the_fix() {
        let (dir, acme, _, port) = two_accounts();

        assert_eq!(exchange(port, "GET", "/api/health", &[], "").status, 200, "the health check needs no token");

        let missing = exchange(port, "GET", "/api/projects", &[], "");
        assert_eq!(missing.status, 401, "{}", missing.body);
        assert_eq!(missing.header("www-authenticate"), Some("Bearer realm=\"parcad\""));
        assert!(missing.body.contains("Authorization: Bearer <token>"), "{}", missing.body);
        assert!(missing.body.contains("brew install parcad"), "the local way out is named: {}", missing.body);

        let wrong = exchange(port, "GET", "/api/projects", &[("Authorization", "Bearer nope")], "");
        assert_eq!(wrong.status, 401);
        assert!(wrong.header("www-authenticate").is_some_and(|c| c.contains("invalid_token")), "{}", wrong.head);
        assert!(wrong.body.contains("not recognised"), "{}", wrong.body);

        let mcp = exchange(port, "POST", "/mcp", &[("Content-Type", "application/json")], "{}");
        assert_eq!(mcp.status, 401, "/mcp is behind the same door: {}", mcp.body);

        let page = exchange(port, "GET", "/", &[("Authorization", &bearer(&acme))], "");
        assert_eq!(page.status, 404, "no frontend is served, token or not");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two accounts save `bracket` and each lists only its own — over `/api`,
    /// whose handlers run inside the middleware's task, and over `/mcp`, which
    /// rmcp serves on a task of its own and reads the tenant off the request.
    #[test]
    fn two_accounts_on_one_host_see_only_their_own_parts() {
        let (dir, acme, bolt, port) = two_accounts();
        let json = ("Content-Type", "application/json");
        let save = |token: &str, name: &str, script: &str| {
            exchange(
                port,
                "PUT",
                &format!("/api/projects/{name}"),
                &[json, ("Authorization", &bearer(token))],
                &serde_json::json!({ "script": script }).to_string(),
            )
        };
        let saved = save(&acme, "bracket", "return box(1, 1, 1);");
        assert_eq!(saved.status, 200, "{}", saved.body);
        let saved: serde_json::Value = serde_json::from_str(&saved.body).unwrap();
        assert_eq!(saved["path"], "bracket.parcad/part.js", "shown relative to the account's root");
        assert_eq!(save(&bolt, "bracket", "return box(2, 2, 2);").status, 200);
        assert_eq!(save(&bolt, "lid", "return box(3, 3, 3);").status, 200);

        let listed = exchange(port, "GET", "/api/projects", &[("Authorization", &bearer(&acme))], "");
        let listed: serde_json::Value = serde_json::from_str(&listed.body).unwrap();
        let names = listed["projects"].as_array().expect("a list");
        assert!(names.contains(&serde_json::json!("bracket")), "{names:?}");
        assert!(!names.contains(&serde_json::json!("lid")), "the other account's part: {names:?}");
        assert_eq!(listed["directory"], ".", "the server's layout is not the account's to see");
        let tenants = dir.join("tenants");
        assert_eq!(std::fs::read_to_string(tenants.join("acme/bracket.parcad/part.js")).unwrap(), "return box(1, 1, 1);");
        assert_eq!(std::fs::read_to_string(tenants.join("bolt/bracket.parcad/part.js")).unwrap(), "return box(2, 2, 2);");

        let list_projects = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {
                "name": "list_projects", "arguments": {},
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        })
        .to_string();
        let over_mcp = |token: &str, host: &str| {
            exchange(
                port,
                "POST",
                "/mcp",
                &[
                    json,
                    ("Accept", "application/json, text/event-stream"),
                    ("MCP-Protocol-Version", "2026-07-28"),
                    ("Mcp-Method", "tools/call"),
                    ("Mcp-Name", "list_projects"),
                    ("Host", host),
                    ("Authorization", &bearer(token)),
                ],
                &list_projects,
            )
        };
        let on_acme = over_mcp(&acme, "cad.example.test");
        assert_eq!(on_acme.status, 200, "{}", on_acme.body);
        assert!(on_acme.body.contains("\"bracket\"") && !on_acme.body.contains("\"lid\""), "{}", on_acme.body);
        assert!(on_acme.body.contains("\"directory\":\".\""), "{}", on_acme.body);
        let on_bolt = over_mcp(&bolt, "cad.example.test");
        assert!(on_bolt.body.contains("\"lid\""), "{}", on_bolt.body);
        assert_eq!(over_mcp(&bolt, "elsewhere.example").status, 403, "rmcp answers only the names it was given");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One MCP `tools/call` over the socket, as a client past its handshake sends it.
    fn call_tool(port: u16, token: &str, tool: &str, arguments: serde_json::Value) -> Reply {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {
                "name": tool, "arguments": arguments,
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {},
                },
            },
        })
        .to_string();
        exchange(
            port,
            "POST",
            "/mcp",
            &[
                ("Content-Type", "application/json"),
                ("Accept", "application/json, text/event-stream"),
                ("MCP-Protocol-Version", "2026-07-28"),
                ("Mcp-Method", "tools/call"),
                ("Mcp-Name", tool),
                ("Host", PUBLIC_HOST),
                ("Authorization", &bearer(token)),
            ],
            &body,
        )
    }

    /// What an account wrote comes back to that account's bearer alone, as
    /// the file it is; another account has no such export, and a name that
    /// is not one is refused naming the rule.
    #[test]
    fn an_account_fetches_its_own_exports_and_nobody_elses() {
        let (dir, acme, bolt, port) = two_accounts();
        let fetch = |token: &str, name: &str| {
            exchange(port, "GET", &format!("/api/exports/{name}"), &[("Authorization", &bearer(token))], "")
        };
        assert_eq!(exchange(port, "GET", "/api/projects", &[("Authorization", &bearer(&acme))], "").status, 200, "the account's folder exists");
        let exports = dir.join("tenants/acme/.exports");
        std::fs::create_dir_all(exports.join("renders")).unwrap();
        let model = b"3MF bytes \xff\x00 not text".to_vec();
        std::fs::write(exports.join("x.3mf"), &model).unwrap();
        std::fs::write(exports.join("my part.stl"), b"stl").unwrap();
        std::fs::write(exports.join("renders/abc-iso.png"), b"png").unwrap();

        let fetched = fetch(&acme, "x.3mf");
        assert_eq!(fetched.status, 200, "{}", fetched.body);
        assert_eq!(fetched.bytes, model, "the bytes as written");
        assert_eq!(fetched.header("content-type"), Some("model/3mf"));
        assert_eq!(fetched.header("content-disposition"), Some("attachment; filename=\"x.3mf\""));
        let render = fetch(&acme, "renders/abc-iso.png");
        assert_eq!(render.status, 200, "{}", render.body);
        assert_eq!(render.header("content-type"), Some("image/png"));
        assert_eq!(render.bytes, b"png");
        let spaced = fetch(&acme, "my%20part.stl");
        assert_eq!((spaced.status, spaced.header("content-type")), (200, Some("model/stl")), "{}", spaced.body);

        let error = |reply: &Reply| serde_json::from_str::<serde_json::Value>(&reply.body).expect("a JSON error")["error"].as_str().expect("a message").to_string();
        let missing = fetch(&bolt, "x.3mf");
        assert_eq!(missing.status, 404, "{}", missing.body);
        assert!(error(&missing).starts_with("no export named \"x.3mf\" exists for this account"), "{}", missing.body);
        assert!(error(&missing).contains("export_part writes one"), "the fix is named: {}", missing.body);
        assert_eq!(fetch(&acme, "renders/none.png").status, 404);
        assert_eq!(fetch(&acme, "renders").status, 404, "a folder is no export");

        for name in ["..", "%2Fetc%2Fpasswd", "renders/../x.3mf", ".hidden.3mf", "a/b/x.3mf", "exports/x.3mf", "x%3A.3mf"] {
            let refused = fetch(&acme, name);
            assert_eq!(refused.status, 422, "{name}: {}", refused.body);
            let message = error(&refused);
            assert!(message.contains("is not the name of an export"), "{name}: {message}");
            assert!(message.contains("one plain file name"), "the rule is named: {name}: {message}");
        }
        assert_eq!(fetch(&acme, "x.3mf/").status, 422, "an empty segment");
        assert_eq!(exchange(port, "GET", "/api/exports/x.3mf", &[], "").status, 401, "no bearer, no file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole loop a hosted client runs: export_part over `/mcp`, then
    /// the reply's `url` fetched with the same bearer gives the file written.
    #[test]
    #[ignore = "needs the kernel worker"]
    fn an_export_over_mcp_is_fetched_by_the_url_its_reply_carries() {
        let (dir, acme, bolt, port) = two_accounts();
        let exported = call_tool(
            port,
            &acme,
            "export_part",
            serde_json::json!({ "script": "return box(10, 20, 30);", "format": "3mf", "filename": "x.3mf" }),
        );
        assert_eq!(exported.status, 200, "{}", exported.body);
        let reply: serde_json::Value = serde_json::from_str(&exported.body).unwrap();
        let written = &reply["result"]["structuredContent"];
        assert_eq!(written["path"], ".exports/x.3mf", "{written}");
        let url = written["url"].as_str().expect("a hosted reply carries the url");
        assert_eq!(url, "https://cad.example.test/api/exports/x.3mf");

        let route = url.strip_prefix("https://cad.example.test").unwrap();
        let fetched = exchange(port, "GET", route, &[("Authorization", &bearer(&acme))], "");
        assert_eq!(fetched.status, 200, "{}", fetched.body);
        assert_eq!(fetched.header("content-type"), Some("model/3mf"));
        assert_eq!(fetched.bytes, std::fs::read(dir.join("tenants/acme/.exports/x.3mf")).unwrap());
        assert_eq!(fetched.bytes.len(), written["bytes"].as_u64().unwrap() as usize);
        assert_eq!(exchange(port, "GET", route, &[("Authorization", &bearer(&bolt))], "").status, 404);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
